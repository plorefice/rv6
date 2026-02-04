//! Architecture-specific functions.

/// RISC-V architecture.
#[cfg(target_arch = "riscv64")]
mod riscv;

// Re-export the architecture-specific modules.
#[cfg(target_arch = "riscv64")]
pub use self::riscv::*;

/// Halt stub used in testing.
///
/// # Safety
///
/// Always safe to call.
#[cfg(not(target_arch = "riscv64"))]
pub fn halt() {}
