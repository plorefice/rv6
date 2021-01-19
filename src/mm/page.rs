/// Length of a page in bytes. Default is 4 KiB.
pub const PAGE_LENGTH: usize = 4096;

/// A trait common to all types of memory addresses.
pub trait Address: Copy + Into<usize> + From<usize> {
    /// Aligns this address to the next page boundary.
    /// The address is returned unchanged if it already lies on a page boundary.
    fn align_to_next_page(self) -> Self {
        // TODO: this should be a static check
        debug_assert!(PAGE_LENGTH.is_power_of_two());

        Self::from((<Self as Into<usize>>::into(self) + PAGE_LENGTH - 1) & !(PAGE_LENGTH - 1))
    }

    /// Aligns this address to the next page boundary.
    /// The address is returned unchanged if it already lies on a page boundary.
    fn align_to_previous_page(self) -> Self {
        // TODO: this should be a static check
        debug_assert!(PAGE_LENGTH.is_power_of_two());

        Self::from(<Self as Into<usize>>::into(self) & !(PAGE_LENGTH - 1))
    }
}

impl Address for usize {}

/// A physical memory address.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PhysicalAddress(usize);

impl PhysicalAddress {
    /// Interprets a pointer-sized integer as a physical address.
    #[inline(always)]
    pub fn new(addr: usize) -> Self {
        Self(addr)
    }
}

impl From<usize> for PhysicalAddress {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

impl From<PhysicalAddress> for usize {
    fn from(addr: PhysicalAddress) -> Self {
        addr.into()
    }
}

impl Address for PhysicalAddress {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_page_alignment() {
        for t in &[
            (0, 0, 0),
            (1, PAGE_LENGTH, 0),
            (42, PAGE_LENGTH, 0),
            (PAGE_LENGTH - 1, PAGE_LENGTH, 0),
            (PAGE_LENGTH, PAGE_LENGTH, PAGE_LENGTH),
            (PAGE_LENGTH + 1, 2 * PAGE_LENGTH, PAGE_LENGTH),
        ] {
            assert_eq!(t.1, t.0.align_to_next_page());
            assert_eq!(t.2, t.0.align_to_previous_page());
        }
    }
}
