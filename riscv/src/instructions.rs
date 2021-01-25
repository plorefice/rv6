//! Special RISC-V instructions.

/// Halts the hart until the next interrupt arrives.
#[inline]
pub fn wfi() {
    unsafe {
        asm!("wfi", options(nostack, nomem));
    }
}

/// Executes the `nop` instructions, which performs no operation (ie. does nothing).
#[inline]
pub fn nop() {
    unsafe {
        asm!("nop", options(nostack, nomem, preserves_flags));
    }
}
