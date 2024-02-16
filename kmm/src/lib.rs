//! Kernel memory management.

#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::ops::{Add, Sub};

pub mod allocator;

/// A physical memory address representable as an integer of type `U`.
pub trait PhysicalAddress<U>: Copy + Clone + Into<U> + AddressOps<U> {}

/// Operations common to physical address implementations.
pub trait AddressOps<U>:
    Align<U>
    + Add<Output = Self>
    + Sub<Output = Self>
    + Add<U, Output = Self>
    + Sub<U, Output = Self>
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
