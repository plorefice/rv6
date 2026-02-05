//! RISC-V user mode memory access interface.

use crate::arch::riscv::registers::{Sstatus, SstatusFlags};

/// Executes the given closure with user memory access enabled.
pub fn with_user_access<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    let sstatus = Sstatus::read();

    // Enable user access
    // SAFETY: enabling SUM is safe since it only expands access permissions
    unsafe { Sstatus::set(sstatus | SstatusFlags::SUM) };

    let ret = f();

    // Disable user access
    // SAFETY: we are restoring previous state
    unsafe { Sstatus::set(sstatus) };

    ret
}
