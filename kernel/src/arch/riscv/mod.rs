//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use mm::phys_to_virt;
pub use uaccess::with_user_access;

pub mod addr;
pub mod earlycon;
pub mod entry;
pub mod instructions;
pub mod irq;
pub mod mm;
pub mod mmu;
pub mod proc;
pub mod registers;
pub mod sbi;
pub mod stackframe;
pub mod time;
pub mod trap;
pub mod uaccess;

/// The architecture-specific ELF loader implementation.
pub type ArchLoaderImpl = mm::elf::RiscvLoader;

/// The architecture-specific user process executor.
pub type ArchUserExecutor = proc::RiscvUserProcessExecutor;

/// The architecture-specific user memory layout.
pub type ArchUserMemoryLayout = proc::RiscvProcessMemoryLayout;

/// Halts execution on the current hart forever.
pub fn halt() -> ! {
    // Disable all interrupts
    irq::local_irq_disable();
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        instructions::wfi();
    }
}
