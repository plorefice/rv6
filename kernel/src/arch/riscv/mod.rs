//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use mm::{iomap, phys_to_virt};
pub use proc::switch_to_process;
pub use uaccess::with_user_access;

mod addr;
mod earlycon;
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

pub mod time; // TODO: should really be private, or at least properly abstracted

/// The page layout used by the RISC-V architecture.
pub type PageLayout = mmu::RiscvPageLayout;

/// The architecture-specific ELF loader implementation.
pub type ArchLoaderImpl = mm::elf::RiscvLoader;

/// The architecture-specific DMA allocator.
pub type ArchDmaAllocator = mm::dma::RiscvDmaAllocator;

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
