use core::{
    alloc::Layout,
    mem::size_of,
    slice,
    sync::atomic::{Ordering, fence},
};

use crate::{
    drivers::virtio::VirtioDev,
    mm::{
        self,
        addr::DmaAddr,
        dma::{self, DmaAllocator},
    },
};

pub struct Virtq {
    idx: u32,
    size: u16,
    phys: DmaAddr,

    descr: &'static mut [VirtqDescriptor],
    avail: &'static mut VirtqAvailable,
    avail_ring: &'static mut [u16],
    used: &'static mut VirtqUsed,
    used_ring: &'static mut [VirtqUsedElem],
    /// Driver cursor into [`Self::used_ring`]; advanced by [`Self::reclaim`].
    last_seen_used: u16,

    first_free: usize,
    free_count: usize,
}

impl Virtq {
    pub fn new(idx: u32, size: u16) -> Self {
        let size = size as usize;
        let page = mm::page_size();

        let vq_desc_sz = size_of::<VirtqDescriptor>() * size;
        let vq_avail_sz = size_of::<VirtqAvailable>() + size_of::<u16>() * (size + 1);
        let vq_used_sz =
            size_of::<VirtqUsed>() + size_of::<VirtqUsedElem>() * size + size_of::<u16>();

        // Legacy virtio: used ring starts at the next Queue Align (page) boundary
        // after the descriptor table and available ring.
        let vq_used_off = (vq_desc_sz + vq_avail_sz + page - 1) & !(page - 1);
        let vq_total_sz = vq_used_off + vq_used_sz;

        let vq_mem = dma::allocator()
            .alloc_raw_zeroed(Layout::from_size_align(vq_total_sz, page).unwrap())
            .expect("dma allocation failed");

        // SAFETY: lots of pointer arithmetics down below, if my calculations are correct
        //         this should be safe
        unsafe {
            let vq_ptr = vq_mem.as_ptr();

            let vq_avail_off = vq_desc_sz;

            let descr = slice::from_raw_parts_mut(vq_ptr as *mut VirtqDescriptor, size);

            // Chain free descriptors together
            for i in 1..size {
                descr[i - 1].next = i as u16;
            }
            descr[size - 1].next = 0;

            let avail = &mut *(vq_ptr.byte_add(vq_avail_off) as *mut VirtqAvailable);
            let avail_ring =
                slice::from_raw_parts_mut((avail as *mut VirtqAvailable).add(1) as *mut u16, size);

            let used = &mut *(vq_ptr.byte_add(vq_used_off) as *mut VirtqUsed);
            let used_ring = slice::from_raw_parts_mut(
                (used as *mut VirtqUsed).add(1) as *mut VirtqUsedElem,
                size,
            );

            Self {
                idx,
                size: size as u16,
                phys: vq_mem.dma_addr(),

                descr,
                avail,
                avail_ring,
                used,
                used_ring,
                last_seen_used: 0,

                first_free: 0,
                free_count: size,
            }
        }
    }

    pub fn pfn(&self) -> u32 {
        (self.phys.as_usize() / mm::page_size()) as u32
    }

    /// Returns completed descriptor chains from the used ring to the free list.
    ///
    /// Must be called after the device signals that it has used buffers (e.g. via
    /// the used-buffer interrupt), otherwise descriptors are leaked and
    /// [`Self::submit`] eventually runs out of free descriptors.
    pub fn reclaim(&mut self) {
        // Ensure we observe used-ring writes from the device.
        fence(Ordering::SeqCst);

        // SAFETY: packed virtq header fields may be unaligned
        let used_idx = unsafe { core::ptr::addr_of!(self.used.idx).read_unaligned() };
        let qsize = self.size as usize;

        while self.last_seen_used != used_idx {
            let chain_head = self.used_ring[self.last_seen_used as usize % qsize].id as usize;

            // Walk the completed chain and count its length. The last descriptor
            // is linked onto the current free list; the chain head becomes the
            // new free-list head, preserving the existing `next` links.
            let mut idx = chain_head;
            let mut chain_len = 0;
            loop {
                chain_len += 1;
                let flags = self.descr[idx].flags;
                let next = self.descr[idx].next as usize;
                if flags & VirtqDescriptor::NEXT == 0 {
                    self.descr[idx].next = self.first_free as u16;
                    break;
                }
                idx = next;
            }

            self.first_free = chain_head;
            self.free_count += chain_len;
            self.last_seen_used = self.last_seen_used.wrapping_add(1);
        }
    }

    pub fn submit<'a, D, I>(&mut self, dev: &D, buffers: I)
    where
        D: VirtioDev,
        I: IntoIterator<Item = &'a VirtqBuffer> + Clone,
    {
        let total_req = buffers.clone().into_iter().count();
        assert!(
            self.free_count >= total_req,
            "virtq: not enough free descriptors ({}/{})",
            self.free_count,
            total_req
        );

        // Take `total_req` descriptors from the head of the free list. Their
        // existing `next` links become the buffer chain submitted to the device.
        let chain_head = self.first_free;
        let mut idx = chain_head;
        for _ in 1..total_req {
            idx = self.descr[idx].next as usize;
        }
        let next_free = self.descr[idx].next as usize;

        self.first_free = next_free;
        self.free_count -= total_req;

        // Prepare the descriptors
        let mut idx = chain_head;
        let mut rem = total_req;

        for (addr, len, write) in buffers.into_iter().map(|&b| match b {
            VirtqBuffer::Readable { addr, len } => (addr, len, false),
            VirtqBuffer::Writeable { addr, len } => (addr, len, true),
        }) {
            self.descr[idx].addr = addr;
            self.descr[idx].len = len as u32;
            self.descr[idx].flags = 0;

            if rem != 1 {
                self.descr[idx].flags |= VirtqDescriptor::NEXT;
            }
            if write {
                self.descr[idx].flags |= VirtqDescriptor::WRITE;
            }

            idx = self.descr[idx].next as usize;
            rem -= 1;
        }

        assert_eq!(rem, 0, "not all descriptors were submitted");
        assert_eq!(idx, self.first_free, "not all descriptors were submitted");

        // Submit the descriptors
        self.avail_ring[self.avail.idx as usize] = chain_head as u16;
        fence(Ordering::SeqCst);

        self.avail.idx = (self.avail.idx + 1) % self.size;
        fence(Ordering::SeqCst);

        dev.notify(self.idx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtqBuffer {
    Readable { addr: DmaAddr, len: usize },
    Writeable { addr: DmaAddr, len: usize },
}

#[repr(C, packed)]
pub struct VirtqDescriptor {
    addr: DmaAddr,
    len: u32,
    flags: u16,
    next: u16,
}

impl VirtqDescriptor {
    /// This descriptor is followed by another via [`Self::next`].
    const NEXT: u16 = 1 << 0;
    /// Device may write to the buffer (device → driver).
    const WRITE: u16 = 1 << 1;
}

#[repr(C, packed)]
pub struct VirtqAvailable {
    flags: u16,
    idx: u16,
}

#[repr(C, packed)]
pub struct VirtqUsed {
    flags: u16,
    idx: u16,
}

#[repr(C, packed)]
pub struct VirtqUsedElem {
    id: u32,
    len: u32,
}
