//! RISC-V-specific DMA allocator implementation.

use core::{alloc::Layout, ptr::NonNull};

use crate::{
    arch::riscv::{
        addr::{DmaAddrExt, PhysAddrExt},
        mm::GFA,
        mmu::PAGE_SIZE,
    },
    mm::{
        addr::{Align, DmaAddr},
        allocator::{Frame, FrameAllocator},
        dma::{DmaAllocError, DmaAllocator, DmaBuf, DmaBufParts, DmaDirection},
    },
};

// Global DMA allocator instance
static ALLOC: RiscvDmaAllocator = RiscvDmaAllocator;

/// Returns a reference to the global DMA allocator.
#[inline]
pub const fn allocator() -> &'static RiscvDmaAllocator {
    &ALLOC
}

/// RISC-V DMA allocator.
#[derive(Debug)]
pub struct RiscvDmaAllocator;

impl DmaAllocator for RiscvDmaAllocator {
    fn alloc_raw<'a>(&'a self, layout: Layout) -> Result<DmaBuf<'a>, DmaAllocError> {
        // Allocate enough frames to cover the requested layout
        let n_pages = layout.size().align_up(PAGE_SIZE) / PAGE_SIZE;
        let frame = GFA.lock().as_mut().unwrap().alloc(n_pages).expect("oom");

        let ptr = NonNull::new(frame.virt() as *mut u8).unwrap();
        let dma_addr = frame.phys().to_dma_addr();

        // SAFETY: by construction
        unsafe {
            Ok(DmaBuf::new_unchecked(
                ptr,
                dma_addr,
                layout.size(),
                layout.align(),
                self,
            ))
        }
    }

    unsafe fn free_raw(&self, parts: DmaBufParts) {
        let phys_addr = parts.dma_addr.to_phys_addr();
        // SAFETY: by construction, the physical address corresponds to a valid frame
        let frame = unsafe { Frame::unmapped(phys_addr) };
        GFA.lock().as_mut().unwrap().free(frame);
    }

    fn sync_for_device(&self, _addr: DmaAddr, _len: usize, _direction: DmaDirection) {
        // no-op, assumes DMA-coherent platform (QEMU virt)
    }

    fn sync_for_cpu(&self, _addr: DmaAddr, _len: usize, _direction: DmaDirection) {
        // no-op, assumes DMA-coherent platform (QEMU virt)
    }
}
