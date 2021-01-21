/// Length of a page in bytes. Default is 4 KiB.
pub const PAGE_LENGTH: usize = 4096;

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
            assert_eq!(t.1, t.0.align_to_next_page(PAGE_LENGTH));
            assert_eq!(t.2, t.0.align_to_previous_page(PAGE_LENGTH));
        }

        for t in &[
            (0, true),
            (1, false),
            (42, false),
            (PAGE_LENGTH - 1, false),
            (PAGE_LENGTH, true),
            (PAGE_LENGTH + 1, false),
        ] {
            assert_eq!(t.1, t.0.is_page_aligned(PAGE_LENGTH));
        }
    }
}
