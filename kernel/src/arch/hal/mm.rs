//! Hardware abstraction layer for memory management.

use crate::mm::addr::{PhysAddr, VirtAddr};

pub mod dma {
    use crate::mm::dma::{DmaAllocator, DmaAllocatorToken};

    #[inline]
    pub fn allocator(_: DmaAllocatorToken) -> &'static impl DmaAllocator {
        imp::allocator()
    }

    mod imp {
        #[cfg(target_arch = "riscv64")]
        #[inline]
        pub fn allocator() -> &'static impl crate::mm::dma::DmaAllocator {
            crate::arch::riscv::mm::dma::allocator()
        }
    }
}

pub mod mmio {
    use crate::mm::mmio::{IoMapper, IoMapperToken};

    #[inline]
    pub fn mapper(_: IoMapperToken) -> &'static impl IoMapper {
        imp::mapper()
    }

    mod imp {
        #[cfg(target_arch = "riscv64")]
        #[inline]
        pub fn mapper() -> &'static impl crate::mm::mmio::IoMapper {
            crate::arch::riscv::mm::mmio::mapper()
        }
    }
}

#[inline]
pub const fn page_size() -> usize {
    imp::page_size()
}

#[inline]
pub unsafe fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    // SAFETY: assuming the caller has upheld the safety contract
    unsafe { imp::phys_to_virt(paddr) }
}

#[inline]
pub fn with_user_access<R>(f: impl FnOnce() -> R) -> R {
    imp::with_user_access(f)
}

mod imp {
    #[cfg(target_arch = "riscv64")]
    pub use riscv::*;

    #[cfg(target_arch = "riscv64")]
    mod riscv {
        use crate::mm::addr::{PhysAddr, VirtAddr};

        #[inline]
        pub const fn page_size() -> usize {
            crate::arch::riscv::mmu::PAGE_SIZE
        }

        #[inline]
        pub unsafe fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
            // SAFETY: assuming the caller has upheld the safety contract
            unsafe { crate::arch::riscv::mm::phys_to_virt(paddr) }
        }

        #[inline]
        pub fn with_user_access<R>(f: impl FnOnce() -> R) -> R {
            crate::arch::riscv::with_user_access(f)
        }
    }
}
