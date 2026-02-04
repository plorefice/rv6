//! Kernel memory management.

pub mod addr;
pub mod allocator;
pub mod mmio;

/// A trait to be implemented by architecture-specific code to provide information about
/// the memory layout of the system.
pub trait ArchPageLayout {
    /// The shift amount for the page size (e.g., 12 for 4KiB pages).
    const SHIFT: usize;

    /// The size of a page in bytes.
    const SIZE: usize = 1 << Self::SHIFT;
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
