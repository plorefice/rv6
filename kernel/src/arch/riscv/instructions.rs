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

/// Executes a supervisor fence, flushing all TLBs.
#[inline]
pub fn sfence_vma() {
    // SAFETY: no memory side effects
    unsafe {
        asm!("sfence.vma", options(nomem, nostack, preserves_flags));
    }
}

/// Executes an instruction cache flush.
#[inline]
pub fn fence_i() {
    // SAFETY: no memory side effects
    unsafe {
        asm!("fence.i", options(nomem, nostack, preserves_flags));
    }
}
