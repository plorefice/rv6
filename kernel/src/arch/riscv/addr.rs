//! Physical and virtual addresses manipulation.

use core::{
    convert::TryFrom,
    fmt,
    ops::{Add, Sub},
};

use crate::{
    arch::riscv::mmu::PAGE_SHIFT,
    mm::addr::{InvalidAddrError, MemoryAddress, PhysAddr, VirtAddr},
};

/// Physical memory address.
impl MemoryAddress for PhysAddr {
    fn new(addr: usize) -> Self {
        Self::try_new(addr)
            .expect("address passed to PhysAddr::new must not contain any data in bits 56 to 63")
    }

    fn try_new(addr: usize) -> Result<Self, InvalidAddrError> {
        if addr >> 56 != 0 {
            Err(InvalidAddrError)
        } else {
            // SAFETY: upper bits are checked
            Ok(unsafe { Self::new_unchecked(addr) })
        }
    }
}

/// RISC-V specific extensions to the `PhysAddr` type.
pub trait PhysAddrExt {
    /// Creates a new physical address from a physical page index.
    ///
    /// # Panics
    ///
    /// Panics if `ppn` is not a valid physical page index for the target ISA.
    fn from_ppn(ppn: usize) -> Self;

    /// Creates a new physical address throwing away the upper bits of the address.
    fn new_truncated(addr: usize) -> Self;

    /// Returns the lowest 12 bits of this address.
    fn page_offset(self) -> usize;

    /// Returns the full page number of this address.
    fn page_index(self) -> usize;

    /// Returns the 9-bit level 0 page table index.
    fn ppn0(self) -> usize;

    /// Returns the 9-bit level 1 page table index.
    fn ppn1(self) -> usize;

    /// Returns the level 2 page table index.
    ///
    /// The size of this field varies depending on the MMU specification.
    fn ppn2(self) -> usize;

    /// Returns the 17-bit level 3 page table index.
    #[cfg(feature = "sv48")]
    fn ppn3(self) -> usize;
}

impl PhysAddrExt for PhysAddr {
    fn from_ppn(ppn: usize) -> Self {
        Self::try_new(ppn << PAGE_SHIFT)
            .expect("index passed to PhysAddr::from_ppn is not a valid page number")
    }

    fn new_truncated(addr: usize) -> Self {
        // SAFETY: upper bits are discarded
        unsafe { Self::new_unchecked((addr << 8) >> 8) }
    }

    fn page_offset(self) -> usize {
        self.as_usize() & 0xfff
    }

    fn page_index(self) -> usize {
        (self.as_usize() >> PAGE_SHIFT) & 0xfff_ffff_ffff
    }

    fn ppn0(self) -> usize {
        (self.as_usize() >> 12) & 0x1ff
    }

    fn ppn1(self) -> usize {
        (self.as_usize() >> 21) & 0x1ff
    }

    fn ppn2(self) -> usize {
        if cfg!(feature = "sv39") {
            (self.as_usize() >> 30) & 0x3ff_ffff
        } else {
            /* feature = "sv48" */
            (self.as_usize() >> 30) & 0x1ff
        }
    }

    #[cfg(feature = "sv48")]
    fn ppn3(self) -> usize {
        (self.as_usize() >> 39) & 0x1ffff
    }
}

/// Virtual memory address.
///
/// The address width depends on the chosen MMU specification.
///  - In Sv32 mode, virtual addresses are 32-bit wide and all bits are used in the translation.
///  - In Sv39 mode, virtual addresses are 64-bit wide but only the lower 39 bits are used by the
///    MMU. Bits 63–39 must all be equal to bit 38, or else a page-fault exception will occur.
///  - In Sv48 mode, virtual addresses are 64-bit wide but only the lower 48 bits are used by the
///    MMU. Bits 63–48 must all be equal to bit 47, or else a page-fault exception will occur.
///
/// The safe methods of this type ensure that the above constraints are met.
impl MemoryAddress for VirtAddr {
    /// Creates a new virtual address.
    ///
    /// # Panics
    ///
    /// Panics if `addr` is not a valid virtual address for the target ISA and MMU specification.
    fn new(addr: usize) -> Self {
        Self::try_new(addr).expect("address passed to VirtAddr::new must be properly sign-extended")
    }

    /// Tries for create a new virtual address.
    ///
    /// This function tries to performs sign extension to make the address canonical.
    /// It succeeds if upper bits are either a correct sign extension or all null.
    /// Else, an error is returned.
    fn try_new(addr: usize) -> Result<Self, InvalidAddrError> {
        let shr = if cfg!(feature = "sv39") { 38 } else { 47 };

        // SAFETY: upper bits are checked
        unsafe {
            match addr >> shr {
                #[cfg(feature = "sv39")]
                0 | 0x3ffffff => Ok(Self::new_unchecked(addr)),
                #[cfg(feature = "sv48")]
                0 | 0x1ffff => Ok(Self::new_unchecked(addr)),
                1 => Ok(Self::new_truncated(addr)),
                _ => Err(InvalidAddrError),
            }
        }
    }
}

/// RISC-V specific extensions to the `VirtAddr` type.
pub trait VirtAddrExt {
    /// Creates a new virtual address, throwing away the upper bits of the address.
    ///
    /// This function performs sign extension to make the address canonical, so upper bits are
    /// overwritten. If you want to check that these bits contain no data, use `new` or `try_new`.
    fn new_truncated(addr: usize) -> Self;

    /// Returns the lowest 12 bits of this address.
    fn page_offset(self) -> usize;

    /// Returns the full page number of this address.
    fn page_index(self) -> usize;

    /// Returns the 9-bit level 0 page table index.
    fn vpn0(self) -> usize;

    /// Returns the 9-bit level 1 page table index.
    fn vpn1(self) -> usize;

    /// Returns the 9-bit level 2 page table index.
    fn vpn2(self) -> usize;

    /// Returns the 9-bit level 3 page table index.
    #[cfg(feature = "sv48")]
    const fn vpn3(self) -> usize;
}

/// RISC-V specific extensions to the `VirtAddr` type.
impl VirtAddrExt for VirtAddr {
    fn new_truncated(addr: usize) -> Self {
        // SAFETY: upper bits are discarded
        unsafe {
            if cfg!(feature = "sv39") {
                Self::new_unchecked(((addr << 25) as isize >> 25) as usize)
            } else {
                /* feature = "sv48" */
                Self::new_unchecked(((addr << 16) as isize >> 16) as usize)
            }
        }
    }

    fn page_offset(self) -> usize {
        self.as_usize() & 0xfff
    }

    fn page_index(self) -> usize {
        if cfg!(feature = "sv39") {
            (self.as_usize() >> PAGE_SHIFT) & 0x7ff_ffff
        } else {
            /* feature = "sv48" */
            (self.as_usize() >> PAGE_SHIFT) & 0xf_ffff_ffff
        }
    }

    fn vpn0(self) -> usize {
        (self.as_usize() >> 12) & 0x1ff
    }

    fn vpn1(self) -> usize {
        (self.as_usize() >> 21) & 0x1ff
    }

    fn vpn2(self) -> usize {
        (self.as_usize() >> 30) & 0x1ff
    }

    #[cfg(feature = "sv48")]
    const fn vpn3(self) -> usize {
        (self.as_usize() >> 39) & 0x1ff
    }
}
