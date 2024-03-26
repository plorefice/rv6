//! Early kernel console.
//!
//! On RISC-V, this module uses the legacy console SBI extensions.

use crate::arch::riscv::sbi;

/// Writes a single char to the debug console.
pub fn put(c: char) {
    sbi::console::put(c as u8)
}
