//! Collection of memory allocators for the kernel.

use crate::mm::addr::PhysAddr;

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
pub struct Frame {
    /// The physical address of the frame.
    paddr: PhysAddr,
    /// The virtual address of the frame.
    ptr: *mut (),
}

impl Frame {
    /// Returns the physical address of the frame.
    pub fn phys(&self) -> PhysAddr {
        self.paddr
    }

    /// Returns the virtual address of the frame.
    pub fn virt(&self) -> *mut () {
        self.ptr
    }
}

/// A trait for page-grained memory allocators.
pub trait FrameAllocator<const N: usize> {
    /// Allocates a memory section of `count` contiguous pages. If no countiguous section
    /// of the specified size can be allocated, `None` is returned.
    fn alloc(&mut self, count: usize) -> Option<Frame>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    fn free(&mut self, frame: Frame);
}
