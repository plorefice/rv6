//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use instructions::wfi;
pub use mm::{iomap, pa_to_va};

mod addr;
mod entry;
mod instructions;
mod mm;
mod mmu;
mod registers;
mod sbi;
mod stackframe;
mod trap;

pub mod earlycon;
pub mod time; // TODO: should really be private, or at least properly abstracted

/// Halts execution on the current hart forever.
pub fn halt() -> ! {
    // Disable all interrupts.
    // SAFETY: we are halting, if something goes wrong, we don't care
    unsafe { Sstatus::clear(SstatusFlags::SIE) };
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        wfi();
    }
}
