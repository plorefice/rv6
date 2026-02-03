//! Early kernel console.
//!
//! On RISC-V, this module uses the legacy console SBI extensions.

use crate::arch::riscv::sbi;

/// Writes a single byte to the debug console.
pub fn put(c: u8) {
    sbi::console::put(c)
}
