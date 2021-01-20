use spin::Mutex;

use self::bitmap::BitmapAllocator;

use super::page::PhysicalAddress;

mod bitmap;

/// A trait for page-grained memory allocators.
pub trait FrameAllocator {
    /// Allocates a memory section of `count` contiguous pages. If no countiguous section
    /// of the specified size can be allocated, `None` is returned.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn allocate(&mut self, count: usize) -> Option<PhysicalAddress>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn free(&mut self, address: PhysicalAddress);
}

/// Global frame allocator (GFA).
pub static mut GFA: Mutex<LockedAllocator<BitmapAllocator>> = Mutex::new(LockedAllocator::new());

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
    unsafe fn allocate(&mut self, count: usize) -> Option<PhysicalAddress> {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.allocate(count)
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
