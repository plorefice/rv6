//! Disables and enables interrupts on RISC-V architectures.

use crate::arch::riscv::registers::{Sstatus, SstatusFlags};

/// Disables interrupts for the current context.
pub fn local_irq_disable() {
    // SAFETY: is it safe tho?
    unsafe { Sstatus::clear(SstatusFlags::SIE) };
}

/// Enables interrupts for the current context.
pub fn local_irq_enable() {
    // SAFETY: is it safe tho?
    unsafe { Sstatus::set(SstatusFlags::SIE) };
}
