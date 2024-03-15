//! Kernel memory management.

use core::ops::{Add, Sub};

// Memory allocators for the kernel.
pub mod allocator;

/// Functions and types for dealing with memory-mapped I/O.
pub mod mmio;

/// A physical memory address representable as an integer of type `U`.
pub trait PhysicalAddress<U>: Copy + Clone + Into<U> + AddressOps<U> {}

/// Operations common to physical address implementations.
pub trait AddressOps<U>:
    Align<U>
    + Add<Output = Self>
    + Sub<Output = Self>
    + Add<U, Output = Self>
    + Sub<U, Output = Self>
    + PartialOrd
    + Sized
{
}

/// A trait for numeric types that can be aligned to a boundary.
pub trait Align<U> {
    /// Aligns address upwards to the specified bound.
    ///
    /// Returns the first address greater or equal than `addr` with alignment `align`.
    fn align_up(&self, align: U) -> Self;

    /// Aligns address downwards to the specified bound.
    ///
    /// Returns the first address lower or equal than `addr` with alignment `align`.
    fn align_down(&self, align: U) -> Self;

    /// Checks whether the address has the specified alignment.
    fn is_aligned(&self, align: U) -> bool;
}

impl PhysicalAddress<u64> for u64 {}

impl AddressOps<u64> for u64 {}

impl Align<u64> for u64 {
    fn align_up(&self, align: u64) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (*self + align - 1) & !(align - 1)
    }

    fn align_down(&self, align: u64) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        *self & !(align - 1)
    }

    fn is_aligned(&self, align: u64) -> bool {
        assert!(align.is_power_of_two(), "Alignment must be a power of two");
        (*self & (align - 1)) == 0
    }
}
