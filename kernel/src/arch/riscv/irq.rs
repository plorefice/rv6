//! Disables and enables interrupts on RISC-V architectures.

use crate::arch::riscv::registers::{Sstatus, SstatusFlags};

/// Disables interrupts for the current context.
pub fn local_irq_disable() {
    // SAFETY: clearing SIE only affects this hart's interrupt delivery.
    unsafe { Sstatus::clear(SstatusFlags::SIE) };
}

/// Enables interrupts for the current context.
pub fn local_irq_enable() {
    // SAFETY: enabling SIE is safe once the caller has restored a consistent IRQ state.
    unsafe { Sstatus::set(SstatusFlags::SIE) };
}
