#[cfg(target_arch = "riscv64")]
pub mod riscv;
#[cfg(target_arch = "riscv64")]
pub use self::riscv::*;

#[cfg(test)]
pub unsafe fn halt() {}
