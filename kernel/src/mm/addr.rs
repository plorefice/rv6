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

pub use dma::*;
pub use phys::*;
pub use virt::*;

mod dma;
mod phys;
mod virt;

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

// Utility implementations for usize
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
