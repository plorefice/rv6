//! This module provides RISC-V specific functions and data structures,
//! and access to various system registers.

#![allow(unused)]

use crate::arch::riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

pub use mm::phys_to_virt;
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
pub type ArchPageLayout = mmu::RiscvPageLayout;

/// The architecture-specific DMA allocator.
pub type ArchDmaAllocator = mm::dma::RiscvDmaAllocator;

/// The architecture-specific I/O mapper.
pub type ArchIoMapper = mm::mmio::RiscvIoMapper;

/// The architecture-specific ELF loader implementation.
pub type ArchLoaderImpl = mm::elf::RiscvLoader;

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
