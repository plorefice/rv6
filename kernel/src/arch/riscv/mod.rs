//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use addr::PAGE_SIZE;
pub use irq::{local_irq_disable, local_irq_enable};
pub use mm::elf::RiscvLoader;
pub use mm::{alloc_contiguous, alloc_contiguous_zeroed, iomap, palloc, phys_to_virt};
pub use proc::switch_to_process;
pub use uaccess::with_user_access;

mod addr;
mod entry;
mod instructions;
mod irq;
mod mm;
mod mmu;
mod proc;
mod registers;
mod sbi;
mod stackframe;
mod trap;
mod uaccess;

pub mod earlycon;
pub mod time; // TODO: should really be private, or at least properly abstracted

/// Halts execution on the current hart forever.
pub fn halt() -> ! {
    // Disable all interrupts
    local_irq_disable();
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        instructions::wfi();
    }
}
