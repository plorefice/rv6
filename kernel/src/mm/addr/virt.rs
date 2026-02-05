use core::{
    fmt,
    ops::{Add, Sub},
};

use crate::mm::addr::{AddressOps, Align, MemoryAddress};

/// A virtual memory address.
///
/// This is a wrapper type around an `usize`, so it is always pointer-sized on any system.
/// We are targeting 64-bit systems only anyway.
///
/// The actual address width depends on the target ISA, and arch-specific code should ensure
/// that only the valid bits are used.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// Creates a new virtual address without checking whether `addr` is a valid address.
    ///
    /// # Safety
    ///
    /// The address may end up representing an invalid address.
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// Returns the inner representation of the address.
    pub const fn as_usize(self) -> usize {
        self.0
    }

    /// Returns the address as a raw pointer of type `T`.
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    /// Returns the address as a mutable raw pointer of type `T`.
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }
}

// Implement traits
impl AddressOps for VirtAddr {}
impl AddressOps<usize> for VirtAddr {}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperExp for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Binary for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Octal for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Pointer for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(self.0 as *const ()), f)
    }
}

impl Add for VirtAddr {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.as_usize() + rhs.as_usize())
    }
}

impl Add<usize> for VirtAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.as_usize() + rhs)
    }
}

impl Sub for VirtAddr {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.as_usize() - rhs.as_usize())
    }
}

impl Sub<usize> for VirtAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.as_usize() - rhs)
    }
}

impl Align<usize> for VirtAddr {
    fn align_up(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new((self.as_usize() + align - 1) & !(align - 1))
    }

    fn align_down(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new(self.as_usize() & !(align - 1))
    }

    fn is_aligned(&self, align: usize) -> bool {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (self.as_usize() & (align - 1)) == 0
    }
}
