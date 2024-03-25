use core::{cell::UnsafeCell, mem};

use crate::{arch, mm::PhysicalAddress};

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
pub struct Regmap {
    base: *mut u8,
    len: usize,
}

impl Regmap {
    /// Maps a range of physical addresses into virtual memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `base` is a valid MMIO address.
    pub unsafe fn new<A: PhysicalAddress<u64>>(base: A, len: u64) -> Self {
        // SAFETY: assuming caller has upheld the safety contract
        let ptr = unsafe { arch::mm::iomap(base, len) };

        Self {
            base: ptr,
            len: len as usize,
        }
    }

    /// Writes a value of type `T` at `offset` bytes from the start of this regmap.
    pub fn write<T>(&self, offset: usize, v: T) {
        assert!(self.len >= offset + mem::size_of::<T>());

        // SAFETY: proper checks are in place to make sure that `ptr` is a valid address for T
        unsafe {
            let ptr = self.base.byte_add(offset) as *mut T;
            assert!(ptr.is_aligned());
            ptr.write_volatile(v)
        }
    }
}
