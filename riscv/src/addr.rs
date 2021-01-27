//! Physical and virtual addresses manipulation.

use core::{
    convert::TryFrom,
    fmt,
    ops::{Add, Sub},
};

use kmm::{AddressOps, Align, PhysicalAddress};

/// Number of bits that an address needs to be shifted to the left to obtain its page number.
pub const PAGE_SHIFT: u64 = 12;

/// Size of a page in bytes.
pub const PAGE_SIZE: u64 = 1 << 12;

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
        Self::try_new(addr)
            .expect("address passed to PhysAddr::new must not contain any data in bits 56 to 63")
    }

    /// Tries for create a new physical address.
    ///
    /// Returns an error if `addr` is not a valid physical address for the target ISA.
    pub fn try_new(addr: u64) -> Result<Self, InvalidAddrError> {
        if addr >> 56 != 0 {
            Err(InvalidAddrError)
        } else {
            Ok(Self(addr))
        }
    }

    /// Creates a new physical address throwing away the upper bits of the address.
    pub const fn new_truncated(addr: u64) -> Self {
        Self((addr << 8) >> 8)
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

    /// Returns the full page number of this address.
    pub fn page_index(self) -> u64 {
        (u64::from(self) >> PAGE_SHIFT) & 0xfff_ffff_ffff
    }

    /// Returns the 9-bit level 0 page table index.
    pub fn ppn0(self) -> u64 {
        (u64::from(self) >> 12) & 0x1ff
    }

    /// Returns the 9-bit level 1 page table index.
    pub fn ppn1(self) -> u64 {
        (u64::from(self) >> 21) & 0x1ff
    }

    /// Returns the level 2 page table index.
    ///
    /// The size of this field varies depending on the MMU specification.
    pub fn ppn2(self) -> u64 {
        if cfg!(target = "sv39") {
            (u64::from(self) >> 30) & 0x3ff_ffff
        } else {
            /* feature = "sv48" */
            (u64::from(self) >> 30) & 0x1ff
        }
    }

    /// Returns the 17-bit level 3 page table index.
    #[cfg(feature = "sv48")]
    pub fn ppn3(self) -> u64 {
        (u64::from(self) >> 39) & 0x1ffff
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
/// The address width depends on the chosen MMU specification.
///  - In Sv32 mode, virtual addresses are 32-bit wide and all bits are used in the translation.
///  - In Sv39 mode, virtual addresses are 64-bit wide but only the lower 39 bits are used by the
///    MMU. Bits 63–39 must all be equal to bit 38, or else a page-fault exception will occur.
///  - In Sv48 mode, virtual addresses are 64-bit wide but only the lower 48 bits are used by the
///    MMU. Bits 63–48 must all be equal to bit 47, or else a page-fault exception will occur.
///
/// The safe methods of this type ensure that the above constraints are met.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// Creates a new virtual address.
    ///
    /// # Panics
    ///
    /// Panics if `addr` is not a valid virtual address for the target ISA and MMU specification.
    /// See [`VirtAddr`] documentation for details.
    pub fn new(addr: usize) -> Self {
        Self::try_new(addr).expect("address passed to VirtAddr::new must be properly sign-extended")
    }

    /// Tries for create a new virtual address.
    ///
    /// This function tries to performs sign extension to make the address canonical.
    /// It succeeds if upper bits are either a correct sign extension or all null.
    /// Else, an error is returned.
    pub fn try_new(addr: usize) -> Result<Self, InvalidAddrError> {
        let shr = if cfg!(feature = "sv39") { 38 } else { 47 };

        match addr >> shr {
            #[cfg(feature = "sv39")]
            0 | 0x3ffffff => Ok(Self(addr)),
            #[cfg(feature = "sv48")]
            0 | 0x1ffff => Ok(Self(addr)),
            1 => Ok(Self::new_truncated(addr)),
            _ => Err(InvalidAddrError),
        }
    }

    /// Creates a new virtual address, throwing away the upper bits of the address.
    ///
    /// This function performs sign extension to make the address canonical, so upper bits are
    /// overwritten. If you want to check that these bits contain no data, use `new` or `try_new`.
    pub const fn new_truncated(addr: usize) -> Self {
        if cfg!(feature = "sv39") {
            VirtAddr(((addr << 25) as isize >> 25) as usize)
        } else {
            /* feature = "sv48" */
            VirtAddr(((addr << 16) as isize >> 16) as usize)
        }
    }

    /// Creates a new virtual address without checking whether `addr` is a valid address.
    ///
    /// # Safety
    ///
    /// The address may end up representing an invalid address.
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
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

    /// Returns the full page number of this address.
    pub fn page_index(self) -> usize {
        if cfg!(feature = "sv39") {
            (usize::from(self) >> PAGE_SHIFT) & 0x7ff_ffff
        } else {
            /* feature = "sv48" */
            (usize::from(self) >> PAGE_SHIFT) & 0xf_ffff_ffff
        }
    }

    /// Returns the 9-bit level 0 page table index.
    pub fn vpn0(self) -> usize {
        (usize::from(self) >> 12) & 0x1ff
    }

    /// Returns the 9-bit level 1 page table index.
    pub fn vpn1(self) -> usize {
        (usize::from(self) >> 21) & 0x1ff
    }

    /// Returns the 9-bit level 2 page table index.
    pub fn vpn2(self) -> usize {
        (usize::from(self) >> 30) & 0x1ff
    }

    /// Returns the 9-bit level 3 page table index.
    #[cfg(feature = "sv48")]
    pub fn vpn3(self) -> usize {
        (usize::from(self) >> 39) & 0x1ff
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
