use riscv::{PhysAddr, VirtAddr};

use crate::{AddressOps, Align, PhysicalAddress};

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
