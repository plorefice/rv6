//! Hardware abstraction layer for process creation and handling.

use crate::proc::{ProcessBuilder, ProcessId};

pub type AddrSpace = imp::AddrSpace;

pub type ProcArchState = imp::ProcState;

#[inline]
pub fn builder() -> impl ProcessBuilder {
    imp::process_builder()
}

/// Switches execution from the current process to the next process.
#[inline]
pub fn switch(current: ProcessId, next: ProcessId) {
    imp::switch(current, next)
}

#[inline]
/// Resumes execution of the specified process.
pub fn resume(pid: ProcessId) -> ! {
    imp::resume(pid)
}

mod imp {
    #[cfg(target_arch = "riscv64")]
    pub use riscv::*;

    #[cfg(target_arch = "riscv64")]
    mod riscv {
        use crate::proc::{ProcessBuilder, ProcessId};

        pub type AddrSpace = crate::arch::riscv::mm::elf::RiscvAddrSpace;
        pub type ProcState = crate::arch::riscv::proc::ProcState;

        #[inline]
        pub fn process_builder() -> impl ProcessBuilder {
            crate::arch::riscv::proc::process_builder()
        }

        #[inline]
        pub fn switch(current: ProcessId, next: ProcessId) {
            crate::arch::riscv::proc::switch_process(current, next)
        }

        #[inline]
        pub fn resume(pid: ProcessId) -> ! {
            crate::arch::riscv::proc::resume_process(pid)
        }
    }
}
