//! Hardware abstraction layer for process creation and handling.

use crate::proc::{ProcessBuilder, ProcessId};

pub type AddrSpace = imp::AddrSpace;

pub type ProcArchState = imp::ProcState;

#[inline]
pub fn builder() -> impl ProcessBuilder {
    imp::process_builder()
}

/// Switch kernel contexts. `None` is the per-hart idle/scheduler context.
#[inline]
pub fn switch(outgoing: Option<ProcessId>, next: Option<ProcessId>) {
    imp::switch(outgoing, next)
}

/// Enters the idle/scheduler loop and never returns.
#[inline]
pub fn enter_scheduler() -> ! {
    imp::enter_scheduler()
}

/// Spawns a kernel thread and enqueues it for scheduling.
#[inline]
pub fn spawn_kthread(entry: fn(usize), arg: usize) -> ProcessId {
    imp::spawn_kthread(entry, arg)
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
        pub fn switch(outgoing: Option<ProcessId>, next: Option<ProcessId>) {
            crate::arch::riscv::proc::switch(outgoing, next)
        }

        #[inline]
        pub fn enter_scheduler() -> ! {
            crate::arch::riscv::proc::enter_scheduler()
        }

        #[inline]
        pub fn spawn_kthread(entry: fn(usize), arg: usize) -> ProcessId {
            crate::arch::riscv::proc::spawn_kthread(entry, arg)
        }
    }
}
