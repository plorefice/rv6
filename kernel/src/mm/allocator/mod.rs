//! Collection of memory allocators.

use crate::mm::PhysicalAddress;

pub use bitmap::BitmapAllocator;
pub use bump::{BumpAllocator, BumpFrameAllocator};

mod bitmap;
mod bump;

/// The error type returned by fallible allocator operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AllocatorError {
    /// The provided address is not properly aligned.
    UnalignedAddress,
    /// The provided page size is not valid.
    InvalidPageSize,
}

/// A physical memory frame allocated using a [`FrameAllocator`].
#[derive(Debug)]
pub struct Frame<A> {
    /// The physical address of the frame.
    pub paddr: A,
    /// The virtual address of the frame.
    pub ptr: *mut (),
}

/// A trait for page-grained memory allocators.
pub trait FrameAllocator<A, const N: u64>
where
    A: PhysicalAddress<u64>,
{
    /// Allocates a memory section of `count` contiguous pages. If no countiguous section
    /// of the specified size can be allocated, `None` is returned.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc(&mut self, count: usize) -> Option<Frame<A>>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn free(&mut self, address: A);
}
