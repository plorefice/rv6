/// RISC-V architecture.
#[cfg(target_arch = "riscv64")]
pub mod riscv;
#[cfg(target_arch = "riscv64")]
pub use self::riscv::*;

/// Halt stub used in testing.
///
/// # Safety
///
/// Always safe to call.
#[cfg(not(target_arch = "riscv64"))]
pub fn halt() {}
