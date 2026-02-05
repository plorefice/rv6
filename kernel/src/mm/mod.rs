//! Kernel memory management.

pub mod addr;
pub mod allocator;
pub mod dma;
pub mod mmio;

/// Returns the size of a page in bytes.
#[inline]
pub const fn page_size() -> usize {
    crate::arch::hal::mm::page_size()
}
