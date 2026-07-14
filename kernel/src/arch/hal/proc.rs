//! Hardware abstraction layer for process creation and handling.

use crate::proc::{ProcessBuilder, ProcessId};

pub type AddrSpace = imp::AddrSpace;

pub type ProcArchState = imp::ProcState;

#[inline]
pub fn builder() -> impl ProcessBuilder {
    imp::process_builder()
}

/// Resumes execution of the specified process.
pub fn resume(pid: ProcessId) -> ! {
    imp::resume(pid)
}

mod imp {
    use crate::proc::{ProcessBuilder, ProcessId};

    #[cfg(target_arch = "riscv64")]
    pub type AddrSpace = crate::arch::riscv::mm::elf::RiscvAddrSpace;

    #[cfg(target_arch = "riscv64")]
    pub type ProcState = crate::arch::riscv::proc::ProcState;

    #[cfg(target_arch = "riscv64")]
    pub fn process_builder() -> impl ProcessBuilder {
        crate::arch::riscv::proc::process_builder()
    }

    #[cfg(target_arch = "riscv64")]
    pub fn resume(pid: ProcessId) -> ! {
        crate::arch::riscv::proc::resume_process(pid)
    }
}
