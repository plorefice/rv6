#[cfg(target_arch = "riscv64")]
pub mod riscv;
#[cfg(target_arch = "riscv64")]
pub use self::riscv::*;

// TODO: find a way to stub this for tests.
#[cfg(not(target_arch = "riscv64"))]
pub unsafe fn halt() {}
