//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use addr::PAGE_SIZE;
pub use irq::{local_irq_disable, local_irq_enable};
pub use mm::{
    alloc_contiguous, alloc_contiguous_zeroed, iomap, pa_to_va, palloc, proc::alloc_process_memory,
};
pub use proc::switch_to_process;

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
