//! Memory address types.
//!
//! This module defines types and traits for physical and virtual memory addresses,
//! along with basic operations on them.
//!
//! The address types are wrappers around `usize`, ensuring pointer-sized representation
//! on any system. However, the actual address width depends on the target ISA, and
//! arch-specific code should ensure that only the valid bits are used.
//!
//! The module also provides traits for address validation and arithmetic operations.
//!
//! # Safety
//!
//! The default implementations of the address types do not enforce any validity checks.
//! As such, only the unchecked constructors are provided, but the [`MemoryAddress`] trait
//! ensures that each architecture implements its own validity checks.

use core::{
    fmt,
    ops::{Add, Sub},
};

use crate::mm::Align;

/// Error type returned by failed address conversions or operations on invalid addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InvalidAddrError;

/// A trait for memory address types.
pub trait MemoryAddress:
    Sized
    + Clone
    + Copy
    + fmt::Debug
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Align<usize>
    + AddressOps<Self>
    + AddressOps<usize>
{
    /// Creates a new address, checking that the address is valid.
    ///
    /// # Panics
    ///
    /// Panics if the address is not valid.
    fn new(addr: usize) -> Self;

    /// Tries for create a new address.
    ///
    /// Returns an error if `addr` is not a valid address for the target ISA.
    fn try_new(addr: usize) -> Result<Self, InvalidAddrError>;
}

/// A trait for arithmetic operations on addresses.
pub trait AddressOps<Rhs = Self>: Add<Rhs, Output = Self> + Sub<Rhs, Output = Self> {}

/// A physical memory address.
///
/// This is a wrapper type around an `usize`, so it is always pointer-sized on any system.
/// We are targeting 64-bit systems only anyway.
///
/// The actual address width depends on the target ISA, and arch-specific code should ensure
/// that only the valid bits are used.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// Creates a new physical address without checking whether `addr` is a valid address.
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
}

// Implement traits
impl AddressOps for PhysAddr {}
impl AddressOps<usize> for PhysAddr {}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperExp for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Binary for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Octal for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Pointer for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(self.0 as *const ()), f)
    }
}

impl Add for PhysAddr {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.as_usize() + rhs.as_usize())
    }
}

impl Add<usize> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.as_usize() + rhs)
    }
}

impl Sub for PhysAddr {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.as_usize() - rhs.as_usize())
    }
}

impl Sub<usize> for PhysAddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.as_usize() - rhs)
    }
}

impl Align<usize> for PhysAddr {
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

impl Align<usize> for usize {
    fn align_up(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (self + align - 1) & !(align - 1)
    }

    fn align_down(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        self & !(align - 1)
    }

    fn is_aligned(&self, align: usize) -> bool {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (self & (align - 1)) == 0
    }
}
