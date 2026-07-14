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
    /// Creates a new frame that is not mapped to any virtual address.
    /// The virtual address is set to null, indicating that the frame is not currently mapped.
    ///
    /// This is useful when you need to represent a physical frame but do not need to reference
    /// it in virtual memory, for example when needing to free a frame back to the allocator
    /// without having a reference to it.
    ///
    /// # Safety
    ///
    /// This is an inherently unsafe operation for anything other than freeing the frame back
    /// to the allocator, and even then, the caller must ensure that no references to the frame
    /// are being used elsewhere in the kernel, as this could lead to undefined behavior.
    pub unsafe fn unmapped(paddr: PhysAddr) -> Self {
        Frame {
            paddr,
            ptr: core::ptr::null_mut(),
        }
    }

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
