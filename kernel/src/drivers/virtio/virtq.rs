use core::{
    alloc::Layout,
    mem::size_of,
    slice,
    sync::atomic::{fence, Ordering},
};

use crate::{
    arch::{self, PAGE_SIZE},
    drivers::virtio::VirtioDev,
};

pub struct Virtq {
    idx: u32,
    size: u16,
    phys: u64,

    descr: &'static mut [VirtqDescriptor],
    avail: &'static mut VirtqAvailable,
    avail_ring: &'static mut [u16],
    _used: &'static mut VirtqUsed,
    _used_ring: &'static mut [VirtqUsedElem],
    _last_seen_used: u16,

    first_free: usize,
    free_count: usize,
}

impl Virtq {
    pub fn new(idx: u32, size: u16) -> Self {
        let size = size as usize;

        let vq_desc_sz = size_of::<VirtqDescriptor>() * size;
        let vq_avail_sz = size_of::<VirtqAvailable>() + size_of::<u16>() * (size + 1);
        let vq_used_sz =
            size_of::<VirtqUsed>() + size_of::<u16>() + size_of::<VirtqUsedElem>() * size;

        // Used ring must be aligned to 4 bytes
        let vq_avail_pad = (vq_desc_sz + vq_avail_sz + 3) & !3;
        let vq_total_sz = vq_desc_sz + vq_avail_sz + vq_used_sz + vq_avail_pad;

        // SAFETY: layout is valid
        let (vq_ptr, vq_pa) = unsafe {
            arch::alloc_contiguous_zeroed(
                Layout::from_size_align(vq_total_sz, PAGE_SIZE as usize).unwrap(),
            )
        };

        // SAFETY: lots of pointer arithmetics down below, if my calculations are correct
        //         this should be safe
        unsafe {
            let vq_avail_off = vq_desc_sz;
            let vq_used_off = vq_avail_off + vq_avail_sz + vq_avail_pad;

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
                phys: vq_pa.into(),

                descr,
                avail,
                avail_ring,
                _used: used,
                _used_ring: used_ring,
                _last_seen_used: 0,

                first_free: 0,
                free_count: size,
            }
        }
    }

    pub fn pfn(&self) -> u32 {
        (self.phys / PAGE_SIZE) as u32
    }

    pub fn submit<'a, D, I>(&mut self, dev: &D, buffers: I)
    where
        D: VirtioDev,
        I: IntoIterator<Item = &'a VirtqBuffer> + Clone,
    {
        let total_req = buffers.clone().into_iter().count();

        // Remove `total_req` descriptors from the free list
        let chain_head = self.first_free;
        let next_link = {
            let mut idx = chain_head;
            for _ in 0..total_req {
                idx = self.descr[idx].next as usize;
            }
            idx
        };

        // Relink the first used descriptor's parent to the new first free one
        self.descr
            .iter_mut()
            .find(|d| d.next as usize == chain_head)
            .unwrap()
            .next = next_link as u16;

        // Update the free list
        self.first_free = next_link;
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
                self.descr[idx].flags |= 0x1;
            }
            if write {
                self.descr[idx].flags |= 0x2;
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
    Readable { addr: u64, len: usize },
    Writeable { addr: u64, len: usize },
}

#[repr(C, packed)]
pub struct VirtqDescriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
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
