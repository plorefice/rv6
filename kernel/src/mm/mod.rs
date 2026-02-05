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
