//! Physical and virtual addresses manipulation.

use core::{
    convert::TryFrom,
    fmt,
    ops::{Add, Sub},
};

use kmm::{AddressOps, Align, PhysicalAddress};

/// Error type returned by failed address conversions or operations on invalid addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InvalidAddrError;

/// A 64-bit physical memory address.
///
/// This is a wrapper type around an `u64`, so it is always 8 bytes, even when compiled on
/// non 64-bit systems. The [`TryFrom`](core::convert::TryFrom) trait can be used
/// for performing conversions between `u64` and `usize`.
///
/// The actual address width depends on the target ISA. For R32 it will be 34-bit long,
/// for R64 it is 56-bit long. Both are encoded as a 64-bit word.
/// The remaining upper bits must be set to zero.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// Creates a new physical address.
    ///
    /// # Panics
    ///
    /// Panics if `addr` is not a valid physical address for the target ISA.
    /// See [`PhysAddr`] documentation for details.
    pub fn new(addr: u64) -> Self {
        #[cfg(target_arch = "riscv64")]
        assert_eq!(addr >> 56, 0);
        #[cfg(target_arch = "riscv32")]
        assert_eq!(addr >> 34, 0);

        Self(addr)
    }

    /// Tries for create a new physical address.
    ///
    /// Returns an error if `addr` is not a valid physical address for the target ISA.
    pub fn try_new(addr: u64) -> Result<Self, InvalidAddrError> {
        let msb = if cfg!(target_arch = "riscv64") {
            56
        } else {
            34
        };

        if addr >> msb != 0 {
            Err(InvalidAddrError)
        } else {
            Ok(Self(addr))
        }
    }

    /// Creates a new physical address throwing away the upper bits of the address.
    pub const fn new_truncated(addr: u64) -> Self {
        if cfg!(target_arch = "riscv64") {
            Self((addr << 8) >> 8)
        } else {
            Self((addr << 30) >> 30)
        }
    }

    /// Creates a new physical address without checking whether `addr` is a valid address.
    ///
    /// # Safety
    ///
    /// The address may end up representing an invalid address.
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }

    /// Returns the integer representation of this address.
    pub fn data(self) -> u64 {
        self.0
    }

    /// Returns the lowest 12 bits of this address.
    pub fn page_offset(self) -> u64 {
        self.0 & 0xfff
    }
}

impl PhysicalAddress<u64> for PhysAddr {}

impl AddressOps<u64> for PhysAddr {}

impl Align<u64> for PhysAddr {
    fn align_up(&self, align: u64) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new((self.data() + align - 1) & !(align - 1))
    }

    fn align_down(&self, align: u64) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new(self.data() & !(align - 1))
    }

    fn is_aligned(&self, align: u64) -> bool {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (self.data() & (align - 1)) == 0
    }
}

impl From<PhysAddr> for u64 {
    fn from(pa: PhysAddr) -> Self {
        pa.0
    }
}

impl TryFrom<u64> for PhysAddr {
    type Error = InvalidAddrError;

    fn try_from(addr: u64) -> Result<Self, Self::Error> {
        Self::try_new(addr)
    }
}

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
        Self::new(self.0 + rhs.0)
    }
}

impl Add<u64> for PhysAddr {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        Self::new(self.0 + rhs)
    }
}

impl Sub for PhysAddr {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.0 - rhs.0)
    }
}

impl Sub<u64> for PhysAddr {
    type Output = Self;

    fn sub(self, rhs: u64) -> Self::Output {
        Self::new(self.0 - rhs)
    }
}

/// A virtual memory address.
///
/// The address width depends on the chosen MMU specification: 4 bytes for Sv32, and 8 bytes for
/// Sv39 and Sv48. The unused bits in Sv39 and Sv48 mode can be freely used by the OS to encode
/// additional information within the address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// Creates a new virtual address.
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    /// Returns the integer representation of this address.
    pub fn data(self) -> usize {
        self.0
    }

    /// Returns the lowest 12 bits of this address.
    pub fn page_offset(self) -> usize {
        self.0 & 0xfff
    }
}

impl Align<usize> for VirtAddr {
    fn align_up(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new((self.data() + align - 1) & !(align - 1))
    }

    fn align_down(&self, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        Self::new(self.data() & !(align - 1))
    }

    fn is_aligned(&self, align: usize) -> bool {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (self.data() & (align - 1)) == 0
    }
}

impl From<VirtAddr> for usize {
    fn from(va: VirtAddr) -> Self {
        va.0
    }
}

impl From<usize> for VirtAddr {
    fn from(addr: usize) -> Self {
        Self::new(addr)
    }
}

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
