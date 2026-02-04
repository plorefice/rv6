//! Functions and types for dealing with memory-mapped I/O.

use core::{
    cell::UnsafeCell,
    mem::{self, align_of},
};

use crate::{arch, mm::addr::PhysAddr};

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
pub struct Regmap {
    base: usize,
    len: usize,
}

impl Regmap {
    /// Maps a range of physical addresses into virtual memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `base` is a valid MMIO address.
    pub unsafe fn new(base: PhysAddr, len: usize) -> Self {
        // SAFETY: assuming caller has upheld the safety contract
        let ptr = unsafe { arch::iomap(base, len) };

        Self {
            base: ptr as usize,
            len,
        }
    }

    /// Writes a value of type `T` at `offset` bytes from the start of this regmap.
    pub fn write<T>(&self, offset: usize, v: T) {
        assert!(self.len >= offset + mem::size_of::<T>());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset) as *mut T;
            assert_eq!(ptr.align_offset(align_of::<T>()), 0);
            ptr.write_volatile(v)
        }
    }

    /// Reads a value of type `T` at `offset` bytes from the start of this regmap.
    pub fn read<T>(&self, offset: usize) -> T {
        assert!(self.len >= offset + mem::size_of::<T>());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset) as *const T;
            assert_eq!(ptr.align_offset(align_of::<T>()), 0);
            ptr.read_volatile()
        }
    }

    /// Reads `buf.len()` bytes starting at `offset` and places them in `buf`.
    pub fn read_bytes(&self, offset: usize, buf: &mut [u8]) {
        assert!(self.len >= offset + buf.len());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = (self.base + offset) as *const u8;

            for (i, b) in buf.iter_mut().enumerate() {
                *b = ptr.add(i).read_volatile();
            }
        }
    }
}
