//! Functions and types for dealing with memory-mapped I/O.

use core::{
    cell::UnsafeCell,
    mem::{self, align_of},
    num::NonZeroUsize,
};

use crate::mm::addr::{PhysAddr, VirtAddr};

/// Error type for I/O mapping operations.
#[derive(Debug)]
pub enum IoMapError {
    /// The requested mapping range is invalid (e.g., zero-length or unaligned).
    InvalidRange,
    /// The requested mapping range exceeds the addressable physical memory.
    OutOfBounds,
    /// The mapping operation failed due to insufficient resources or other reasons.
    MappingFailed,
}

/// A trait for mapping physical memory regions into the kernel's virtual address space for MMIO access.
pub trait IoMapper: Send + Sync {
    /// Maps a range of physical addresses into virtual memory and returns a pointer to the mapped region.
    fn iomap(&self, base: PhysAddr, len: NonZeroUsize) -> Result<IoMapping, IoMapError>;

    /// Unmaps a previously mapped region of virtual memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `mapping` is a valid mapping returned by this mapper
    /// and must not use the mapping after it has been unmapped.
    unsafe fn iounmap(&self, mapping: IoMapping);
}

// Token type to ensure that only the HAL code can create allocators.
pub(crate) struct IoMapperToken(());

/// Returns a reference to the architecture-specific IO mapper.
#[inline]
pub fn mapper() -> &'static impl IoMapper {
    crate::arch::hal::mm::mmio::mapper(IoMapperToken(()))
}

/// Read-only register.
#[repr(transparent)]
pub struct RO<T>
where
    T: Copy,
{
    register: VolatileCell<T>,
}

impl<T> RO<T>
where
    T: Copy,
{
    /// Reads a value from the register.
    #[inline(always)]
    pub fn read(&self) -> T {
        self.register.get()
    }
}

/// Read-write register.
#[repr(transparent)]
pub struct RW<T>
where
    T: Copy,
{
    register: VolatileCell<T>,
}

impl<T> RW<T>
where
    T: Copy,
{
    /// Performs a read-modify-write operation on the register.
    ///
    /// # Safety
    ///
    /// Unsafe because writes to registers are side-effectful.
    #[inline(always)]
    pub fn modify<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        self.register.set(f(self.register.get()));
    }

    /// Reads a value from the register.
    #[inline(always)]
    pub fn read(&self) -> T {
        self.register.get()
    }

    /// Writes a value into the register.
    ///
    /// # Safety
    ///
    /// Unsafe because writes to registers are side-effectful.
    #[inline(always)]
    pub unsafe fn write(&self, val: T) {
        self.register.set(val)
    }
}

/// Wrapper around an [`UnsafeCell`] that performs volatile operations.
#[repr(transparent)]
pub struct VolatileCell<T> {
    inner: UnsafeCell<T>,
}

impl<T> VolatileCell<T> {
    /// Creates a new `VolatileCelle` containing the given value.
    pub const fn new(val: T) -> Self {
        Self {
            inner: UnsafeCell::new(val),
        }
    }

    /// Reads the content of the cell.
    #[inline(always)]
    pub fn get(&self) -> T
    where
        T: Copy,
    {
        // SAFETY: same considerations as [`ptr::read_volatile`].
        unsafe { self.inner.get().read_volatile() }
    }

    /// Writes the given value into the cell.
    #[inline(always)]
    pub fn set(&self, val: T)
    where
        T: Copy,
    {
        // SAFETY: same considerations as [`ptr::write_volatile`].
        unsafe { self.inner.get().write_volatile(val) }
    }
}

/// A MMIO region mapped into virtual memory.
#[derive(Debug)]
pub struct IoMapping {
    base: VirtAddr,
    size: NonZeroUsize,
    pa: PhysAddr,
}

impl IoMapping {
    /// Creates a new `IoMapping` with the given parameters.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the provided `base` and `size` correspond to a valid mapping
    /// of the physical address `pa`.
    pub unsafe fn new_unchecked(base: VirtAddr, size: NonZeroUsize, pa: PhysAddr) -> Self {
        Self { base, size, pa }
    }

    /// Returns the virtual address corresponding to the start of this mapping.
    pub fn virt(&self) -> VirtAddr {
        self.base
    }

    /// Returns the size of this mapping in bytes.
    pub fn size(&self) -> NonZeroUsize {
        self.size
    }

    /// Returns the physical address corresponding to the start of this mapping.
    pub fn phys(&self) -> PhysAddr {
        self.pa
    }

    /// Writes a value of type `T` at `offset` bytes from the start of this regmap.
    pub fn write<T>(&self, offset: usize, v: T) {
        assert!(self.size.get() >= offset + mem::size_of::<T>());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset).as_mut_ptr::<T>();
            assert_eq!(ptr.align_offset(align_of::<T>()), 0);
            ptr.write_volatile(v)
        }
    }

    /// Reads a value of type `T` at `offset` bytes from the start of this regmap.
    pub fn read<T>(&self, offset: usize) -> T {
        assert!(self.size.get() >= offset + mem::size_of::<T>());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset).as_ptr::<T>();
            assert_eq!(ptr.align_offset(align_of::<T>()), 0);
            ptr.read_volatile()
        }
    }

    /// Reads `buf.len()` bytes starting at `offset` and places them in `buf`.
    pub fn read_bytes(&self, offset: usize, buf: &mut [u8]) {
        assert!(self.size.get() >= offset + buf.len());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset).as_ptr::<u8>();
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ptr.add(i).read_volatile();
            }
        }
    }
}
