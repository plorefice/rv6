//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

use crate::arch::riscv::{
    instructions::wfi,
    registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags},
};

pub use addr::{PhysAddr, VirtAddr};

pub mod addr;
pub mod entry;
pub mod instructions;
pub mod memory;
pub mod mmu;
pub mod registers;
pub mod sbi;
pub mod stackframe;
pub mod time;
pub mod trap;

/// Halts execution on the current hart forever.
pub fn halt() -> ! {
    // Disable all interrupts.
    unsafe { Sstatus::clear(SstatusFlags::SIE) };
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        wfi();
    }
}
