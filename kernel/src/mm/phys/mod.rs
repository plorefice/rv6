use core::mem::size_of;

use spin::Mutex;

use self::bitmap::BitmapAllocator;

use super::page::{Address, PAGE_SIZE};

mod address;
pub mod bitmap;

pub use address::*;

/// The error type returned by fallible allocator operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AllocatorError {
    /// The provided address is not properly aligned.
    UnalignedAddress,
    /// The provided page size is not valid.
    InvalidPageSize,
}

/// A trait for page-grained memory allocators.
pub trait FrameAllocator {
    /// Allocates a memory section of `count` contiguous pages. If no countiguous section
    /// of the specified size can be allocated, `None` is returned.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysicalAddress>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn free(&mut self, address: PhysicalAddress);

    /// Same as [`alloc`], but the allocated memory is also zeroed after allocation.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc_zeroed(&mut self, count: usize) -> Option<PhysicalAddress> {
        let paddr = Self::alloc(self, count)?;
        let uaddr = usize::from(paddr);

        for i in 0..PAGE_SIZE / size_of::<usize>() {
            (uaddr as *mut usize).add(i).write(0);
        }

        Some(paddr)
    }
}

/// Global frame allocator (GFA).
pub static mut GFA: LockedAllocator<BitmapAllocator> = LockedAllocator::new();

/// A frame allocator wrapped in a [`Mutex`] for concurrent access.
pub struct LockedAllocator<T> {
    inner: Mutex<Option<T>>,
}

impl<T> LockedAllocator<T> {
    const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl<T> FrameAllocator for LockedAllocator<T>
where
    T: FrameAllocator,
{
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysicalAddress> {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.alloc(count)
        } else {
            None
        }
    }

    unsafe fn free(&mut self, address: PhysicalAddress) {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.free(address);
        }
    }
}

/// Initializes a physical memory allocator on the specified memory range.
///
/// # Safety
///
/// There can be no guarantee that the memory being initialized isn't already in use by the system.
pub unsafe fn init(mem_start: PhysicalAddress, mem_size: usize) -> Result<(), AllocatorError> {
    let mem_start = mem_start.align_to_next_page(PAGE_SIZE);
    let mem_end = (mem_start + mem_size).align_to_previous_page(PAGE_SIZE);

    *GFA.inner.lock() = Some(BitmapAllocator::init(mem_start, mem_end, PAGE_SIZE)?);

    Ok(())
}
