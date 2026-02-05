//! Hardware abstraction layer for CPU-specific functionality.

#[inline]
pub fn halt() -> ! {
    imp::halt()
}

#[inline]
pub fn get_cycles() -> u64 {
    imp::get_cycles()
}

#[inline]
pub fn cycles_per_sec() -> u64 {
    imp::cycles_per_sec()
}

mod imp {
    #[cfg(target_arch = "riscv64")]
    pub use riscv::*;

    #[cfg(target_arch = "riscv64")]
    mod riscv {
        #[inline]
        pub fn halt() -> ! {
            crate::arch::riscv::halt()
        }

        #[inline]
        pub fn get_cycles() -> u64 {
            crate::arch::riscv::time::get_cycles()
        }

        #[inline]
        pub fn cycles_per_sec() -> u64 {
            crate::arch::riscv::time::CLINT_TIMEBASE
        }
    }
}
