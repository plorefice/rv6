//! Stack unwinding for RISC-V.
//!
//! **IMPORTANT:** in order for unwinding to work, the kernel must be compiled using `rustc`'s
//! `force-frame-pointers=yes` option.

use crate::{
    arch::riscv::mm::{KERNEL_BASE, KSTACK_MEM_SIZE},
    ksyms,
    mm::addr::MemoryAddress,
};

/// Structure of a stack frame on RISC-V.
struct StackFrame {
    fp: usize,
    ra: usize,
}

/// Upper bound on the number of frames printed, in case the chain loops.
const MAX_FRAMES: usize = 64;

/// Unwinds and prints the current stack frame.
pub fn unwind_stack_frame() {
    kprintln!("Call trace:");
    walk_stack_frame();
}

/// Traverses the stack frame and prints the call stack.
fn walk_stack_frame() {
    let mut fp: usize;

    // SAFETY: no side effects
    unsafe { core::arch::asm!("add {}, fp, zero", out(reg) fp) };

    let mut pc = walk_stack_frame as *const fn() as usize;
    let mut prev_fp = 0;

    for _ in 0..MAX_FRAMES {
        if !is_kernel_text_address(pc) {
            break;
        }

        print_trace_address(pc);

        // The outermost frame of a task (eg. `kthread_trampoline`) has a zero saved `fp`
        // while its `ra` still points into kernel text, so the chain must be validated
        // before dereferencing: faulting here would re-enter the trap handler.
        if !is_valid_frame_pointer(fp, prev_fp) {
            break;
        }

        // SAFETY: `fp` was checked to be an aligned kernel address within the current stack.
        let frame = unsafe { &*(fp as *const StackFrame).sub(1) };
        prev_fp = fp;
        fp = frame.fp;
        pc = frame.ra;
    }
}

/// Returns whether `fp` can be dereferenced as the frame pointer of the next frame up.
///
/// Stacks grow downwards, so a caller's frame always sits at a higher address than the
/// callee's, within the same (at most [`KSTACK_MEM_SIZE`]-sized) kernel stack.
fn is_valid_frame_pointer(fp: usize, prev_fp: usize) -> bool {
    fp >= KERNEL_BASE.as_usize()
        && fp.is_multiple_of(align_of::<StackFrame>())
        && fp > prev_fp
        && (prev_fp == 0 || fp - prev_fp <= KSTACK_MEM_SIZE)
}

/// Returns whether an address lies withing the kernel's `.text` section.
fn is_kernel_text_address(pc: usize) -> bool {
    unsafe extern "C" {
        static _stext: usize;
        static _etext: usize;
    }

    // SAFETY: _stext and _etext are initialized by the linker
    unsafe { pc >= (&_stext as *const _ as usize) && pc <= (&_etext as *const _ as usize) }
}

/// Traces the function to which PC belongs and displays both its name and the offset within.
fn print_trace_address(pc: usize) {
    kprint!(" [<{:016x}>] ", pc);
    if let Some((sym, off)) = ksyms::resolve_symbol(pc) {
        kprintc!("<{}>+0x{:x}", sym, off);
    } else {
        kprintc!("?");
    }
    kprinte!();
}
