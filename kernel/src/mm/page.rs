/// Length in bits of the offset part of the page.
pub const PAGE_SHIFT: usize = 12;

/// Length of a page in bytes.
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

/// Bitmask of the page number part of an address.
pub const PAGE_MASK: usize = !(PAGE_SIZE - 1);

/// A trait common to all types of memory addresses.
pub trait Address: Copy + Into<usize> + From<usize> {
    /// Aligns this address to the next page boundary.
    /// The address is returned unchanged if it already lies on a page boundary.
    fn align_to_next_page(self, page_size: usize) -> Self {
        // TODO: this should be a static check
        debug_assert!(page_size.is_power_of_two());

        Self::from((self.into() + page_size - 1) & !(page_size - 1))
    }

    /// Aligns this address to the next page boundary.
    /// The address is returned unchanged if it already lies on a page boundary.
    fn align_to_previous_page(self, page_size: usize) -> Self {
        // TODO: this should be a static check
        debug_assert!(page_size.is_power_of_two());

        Self::from(self.into() & !(page_size - 1))
    }

    /// Returns true if this address is aligned to the page boundary.
    fn is_page_aligned(self, page_size: usize) -> bool {
        // TODO: this should be a static check
        debug_assert!(page_size.is_power_of_two());

        (self.into() & (page_size - 1)) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // For testing purposes
    impl Address for usize {}

    #[test_case]
    fn address_page_alignment() {
        for t in &[
            (0, 0, 0),
            (1, PAGE_SIZE, 0),
            (42, PAGE_SIZE, 0),
            (PAGE_SIZE - 1, PAGE_SIZE, 0),
            (PAGE_SIZE, PAGE_SIZE, PAGE_SIZE),
            (PAGE_SIZE + 1, 2 * PAGE_SIZE, PAGE_SIZE),
        ] {
            assert_eq!(t.1, t.0.align_to_next_page(PAGE_SIZE));
            assert_eq!(t.2, t.0.align_to_previous_page(PAGE_SIZE));
        }

        for t in &[
            (0, true),
            (1, false),
            (42, false),
            (PAGE_SIZE - 1, false),
            (PAGE_SIZE, true),
            (PAGE_SIZE + 1, false),
        ] {
            assert_eq!(t.1, t.0.is_page_aligned(PAGE_SIZE));
        }
    }
}
