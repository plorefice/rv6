//! Disables and enables interrupts on RISC-V architectures.

use core::arch::asm;

use crate::arch::{
    hal::cpu::IrqFlags,
    riscv::registers::{Sstatus, SstatusFlags},
};

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

#[inline]
pub fn local_irq_save() -> IrqFlags {
    let prev: u64;

    // Atomically: read sstatus, clear SIE, return old SIE bit.
    // SAFETY: only clears local SIE; does not touch SUM/FS/SPIE/etc.
    unsafe {
        asm!(
            "csrrc {prev}, sstatus, {sie}",
            prev = out(reg) prev,
            sie = in(reg) SstatusFlags::SIE.bits(),
            options(nostack, preserves_flags)
        );
    }

    let prev = SstatusFlags::from_bits_truncate(prev);
    IrqFlags::from_raw((prev & SstatusFlags::SIE).bits())
}
#[inline]
pub fn local_irq_restore(flags: IrqFlags) {
    // SAFETY: restores only SIE; does not touch SUM/FS/SPIE/etc.
    unsafe {
        if flags.to_raw() & SstatusFlags::SIE.bits() != 0 {
            Sstatus::set(SstatusFlags::SIE);
        } else {
            Sstatus::clear(SstatusFlags::SIE);
        }
    }
}
