//! Kernel context switching.

use crate::arch::riscv::proc::Context;

unsafe extern "C" {
    /// Saves callee-saved registers into `old`, switches to `satp` / `tp`,
    /// restores registers from `new`, and returns into that context.
    fn swtch(old: *mut Context, new: *const Context, satp: usize, tp: usize);
}

/// Switches from the current kernel thread to another.
///
/// On return, execution continues in the task described by `new` (or, when that
/// task later switches back, at the instruction after this call).
///
/// `new_satp` and `new_tp` are installed after the outgoing stack pointer is
/// saved and before the incoming one is loaded, so they must describe the
/// address space that owns `new.sp`.
///
/// # Safety
///
/// - `current` and `new` must be valid, non-aliased context slots for the
///   outgoing and incoming tasks.
/// - `new_satp` must be a valid SATP value for the incoming task.
/// - `new_tp` must point at that task's [`crate::arch::riscv::proc::ThreadInfo`].
/// - The caller must not hold locks that the incoming task might need.
pub unsafe fn switch_context(
    current: *mut Context,
    new: *const Context,
    new_satp: usize,
    new_tp: usize,
) {
    // SAFETY: caller guarantees valid contexts, SATP, and thread-info pointer.
    unsafe { swtch(current, new, new_satp, new_tp) };
}
