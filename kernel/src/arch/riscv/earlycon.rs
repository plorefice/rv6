//! Early kernel console.
//!
//! On RISC-V, this module uses the legacy console SBI extensions.

use crate::{arch::riscv::sbi, drivers::earlycon::EarlyCon};

/// The RISC-V early console implementation.
pub struct RiscvEarlyCon;

impl EarlyCon for RiscvEarlyCon {
    fn put(&self, byte: u8) {
        sbi::console::put(byte)
    }
}

/// Registers the RISC-V early console.
pub fn register() {
    crate::drivers::earlycon::register(&RiscvEarlyCon);
}
