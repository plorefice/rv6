//! Architecture-specific functions.

pub(crate) mod hal;

/// RISC-V architecture.
#[cfg(target_arch = "riscv64")]
mod riscv;
