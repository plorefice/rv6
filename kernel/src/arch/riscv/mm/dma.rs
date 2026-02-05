//! RISC-V-specific DMA allocator implementation.

use core::{
    alloc::Layout,
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

use crate::{
    arch::riscv::{addr::PhysAddrExt, mm::GFA, mmu::PAGE_SIZE},
    mm::{
        addr::{Align, DmaAddr},
        allocator::FrameAllocator,
        dma::{DmaAllocError, DmaAllocator, DmaBuf, DmaDirection, DmaObject, DmaSafe},
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
    fn alloc_raw(&self, layout: Layout) -> Result<DmaBuf, DmaAllocError> {
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
            ))
        }
    }

    unsafe fn free_raw(&self, buf: DmaBuf) {
        todo!()
    }

    fn sync_for_device(&self, addr: DmaAddr, len: usize, direction: DmaDirection) {
        todo!()
    }

    fn sync_for_cpu(&self, addr: DmaAddr, len: usize, direction: DmaDirection) {
        todo!()
    }
}
