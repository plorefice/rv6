//! Physical and virtual addresses manipulation.

use core::{
    convert::TryFrom,
    fmt,
    ops::{Add, Sub},
};

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

/// A 64-bit virtual memory address.
///
/// This is a wrapper type around an `u64`, so it is always 8 bytes, even when compiled on
/// non 64-bit systems. The [`TryFrom`](core::convert::TryFrom) trait can be used
/// for performing conversions between `u64` and `usize`.
///
/// The actual address width depends on the chosen MMU specification: 32 bits for Sv32, 39 bits
/// for Sv39 and 48 bits for Sv48. The remaining bits can be freely used by the OS to encode
/// additional information within the address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(u64);

/// Error type returned by failed address conversions or operations on invalid addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InvalidAddrError;

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

    /// Aligns address upwards to the specified bound.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two or the result is not a valid physical address.
    pub fn align_up<U>(self, align: U) -> Self
    where
        U: Into<u64>,
    {
        Self::new(align_up(self.0, align.into()))
    }

    /// Aligns address downwards to the specified bound.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two
    pub fn align_down<U>(self, align: U) -> Self
    where
        U: Into<u64>,
    {
        Self::new(align_down(self.0, align.into()))
    }

    /// Checks whether the address has the specified alignment.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two
    pub fn is_aligned<U>(self, align: U) -> bool
    where
        U: Into<u64>,
    {
        is_aligned(self.0, align.into())
    }

    /// Returns the lowest 12 bits of this address.
    pub fn page_offset(self) -> u64 {
        self.0 & 0xfff
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

impl VirtAddr {
    /// Creates a new virtual address.
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Aligns address upwards to the specified bound.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two or the result is not a valid physical address.
    pub fn align_up<U>(self, align: U) -> Self
    where
        U: Into<u64>,
    {
        Self::new(align_up(self.0, align.into()))
    }

    /// Aligns address downwards to the specified bound.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two
    pub fn align_down<U>(self, align: U) -> Self
    where
        U: Into<u64>,
    {
        Self::new(align_down(self.0, align.into()))
    }

    /// Checks whether the address has the specified alignment.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two
    pub fn is_aligned<U>(self, align: U) -> bool
    where
        U: Into<u64>,
    {
        is_aligned(self.0, align.into())
    }

    /// Returns the lowest 12 bits of this address.
    pub fn page_offset(self) -> u64 {
        self.0 & 0xfff
    }
}

impl From<VirtAddr> for u64 {
    fn from(va: VirtAddr) -> Self {
        va.0
    }
}

impl From<u64> for VirtAddr {
    fn from(addr: u64) -> Self {
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

/// Aligns address upwards to the specified bound.
///
/// Returns the first address greater or equal than `addr` with alignment `align`.
///
/// # Panics
///
/// Panics if the alignment is not a power of two.
#[inline]
fn align_up(addr: u64, align: u64) -> u64 {
    assert!(align.is_power_of_two(), "Alignment must be a power of two");
    (addr + align - 1) & !(align - 1)
}

/// Aligns address downwards to the specified bound.
///
/// Returns the first address lower or equal than `addr` with alignment `align`.
///
/// # Panics
///
/// Panics if the alignment is not a power of two.
#[inline]
fn align_down(addr: u64, align: u64) -> u64 {
    assert!(align.is_power_of_two(), "Alignment must be a power of two");
    addr & !(align - 1)
}

/// Checks whether the address has the specified alignment.
///
/// # Panics
///
/// Panics if the alignment is not a power of two.
#[inline]
fn is_aligned(addr: u64, align: u64) -> bool {
    assert!(align.is_power_of_two(), "Alignment must be a power of two");
    (addr & (align - 1)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment() {
        const ALIGN: u64 = 4096;

        for t in &[
            (0, 0, 0),
            (1, ALIGN, 0),
            (42, ALIGN, 0),
            (ALIGN - 1, ALIGN, 0),
            (ALIGN, ALIGN, ALIGN),
            (ALIGN + 1, 2 * ALIGN, ALIGN),
        ] {
            assert_eq!(t.1, align_up(t.0, ALIGN));
            assert_eq!(t.2, align_down(t.0, ALIGN));
        }

        for t in &[
            (0, true),
            (1, false),
            (42, false),
            (ALIGN - 1, false),
            (ALIGN, true),
            (ALIGN + 1, false),
        ] {
            assert_eq!(t.1, is_aligned(t.0, ALIGN));
        }
    }
}
