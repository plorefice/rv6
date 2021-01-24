use riscv::PhysAddr;
use spin::Mutex;

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
pub trait FrameAllocator<const N: u64> {
    /// Allocates a memory section of `count` contiguous pages. If no countiguous section
    /// of the specified size can be allocated, `None` is returned.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysAddr>;

    /// Releases the allocated memory starting at the specified address back to the kernel.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn free(&mut self, address: PhysAddr);

    /// Same as [`alloc`], but the allocated memory is also zeroed after allocation.
    ///
    /// # Safety
    ///
    /// Low-level memory twiddling doesn't provide safety guarantees.
    unsafe fn alloc_zeroed(&mut self, count: usize) -> Option<PhysAddr> {
        let paddr = Self::alloc(self, count)?;
        let uaddr: u64 = paddr.into();

        for i in 0..N / 8 {
            (uaddr as *mut u64).add(i as usize).write(0);
        }

        Some(paddr)
    }
}

/// A frame allocator wrapped in a [`Mutex`] for concurrent access.
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

impl<T, const N: u64> FrameAllocator<N> for LockedAllocator<T>
where
    T: FrameAllocator<N>,
{
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysAddr> {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.alloc(count)
        } else {
            None
        }
    }

    unsafe fn free(&mut self, address: PhysAddr) {
        let mut inner = self.inner.lock();

        if let Some(allocator) = &mut *inner {
            allocator.free(address);
        }
    }
}
