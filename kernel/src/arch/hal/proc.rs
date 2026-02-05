//! Hardware abstraction layer for process creation and handling.

use crate::proc::ProcessBuilder;

#[inline]
pub fn builder() -> impl ProcessBuilder {
    imp::process_builder()
}

mod imp {
    use crate::proc::ProcessBuilder;

    #[cfg(target_arch = "riscv64")]
    pub fn process_builder() -> impl ProcessBuilder {
        crate::arch::riscv::proc::process_builder()
    }
}
