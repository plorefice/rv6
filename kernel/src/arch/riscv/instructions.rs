//! Special RISC-V instructions.

use core::arch::asm;

/// Halts the hart until the next interrupt arrives.
#[inline]
pub fn wfi() {
    // SAFETY: `wfi` has no side effects
    unsafe {
        asm!("wfi", options(nostack, nomem));
    }
}

/// Executes the `nop` instructions, which performs no operation (ie. does nothing).
#[inline]
pub fn nop() {
    // SAFETY: `nop` has no side effects
    unsafe {
        asm!("nop", options(nostack, nomem, preserves_flags));
    }
}
