//! Collection of memory allocators.

use spin::Mutex;

use crate::PhysicalAddress;

pub mod bitmap;

/// The error type returned by fallible allocator operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AllocatorError {
    /// The provided address is not properly aligned.
    UnalignedAddress,
    /// The provided page size is not valid.
    InvalidPageSize,
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
    unsafe fn alloc(&mut self, count: usize) -> Option<A>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn free(&mut self, address: A);

    /// Same as [`alloc`], but the allocated memory is also zeroed after allocation.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc_zeroed(&mut self, count: usize) -> Option<A> {
        let paddr = Self::alloc(self, count)?;
        let uaddr: u64 = paddr.into();

        for i in 0..N / 8 {
            (uaddr as *mut u64).add(i as usize).write(0);
        }

        Some(paddr)
    }
}

/// A frame allocator wrapped in a [`Mutex`] for concurrent access.
#[derive(Debug)]
pub struct LockedAllocator<T> {
    inner: Mutex<Option<T>>,
}

impl<T> LockedAllocator<T> {
    /// Creates a new empty locked allocator.
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Configures the underlying allocator to be used.
    pub fn set_allocator(&self, inner: T) {
        *self.inner.lock() = Some(inner);
    }
}

impl<A, T, const N: u64> FrameAllocator<A, N> for LockedAllocator<T>
where
    A: PhysicalAddress<u64>,
    T: FrameAllocator<A, N>,
{
    unsafe fn alloc(&mut self, count: usize) -> Option<A> {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.alloc(count)
        } else {
            None
        }
    }

    unsafe fn free(&mut self, address: A) {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.free(address);
        }
    }
}