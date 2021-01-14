use core::cell::UnsafeCell;

/// Read-only register.
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

/// Wrapper around a [`Cell`] that performs volatile operations.
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
        unsafe { self.inner.get().read_volatile() }
    }

    /// Writes the given value into the cell.
    #[inline(always)]
    pub fn set(&self, val: T)
    where
        T: Copy,
    {
        unsafe { self.inner.get().write_volatile(val) }
    }
}
