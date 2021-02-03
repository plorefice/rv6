//! Stack unwinding for RISC-V.
//!
//! **IMPORTANT:** in order for unwinding to work, the kernel must be compiled using `rustc`'s
//! `force-frame-pointers=yes` option.

use crate::ksyms;

/// Structure of a stack frame on RISC-V.
struct StackFrame {
    fp: usize,
    ra: usize,
}

/// Unwinds and prints the current stack frame.
///
/// # Panics
///
/// This function may panic if the stack frame is corrupted, eg. due to a misaligned frame pointer.
pub fn unwind_stack_frame() {
    kprintln!("Call trace:");
    walk_stack_frame();
}

/// Traverses the stack frame and prints the call stack.
fn walk_stack_frame() {
    let mut fp: usize;
    unsafe { core::arch::asm!("add {}, fp, zero", out(reg) fp) };

    let mut pc = walk_stack_frame as *const fn() as usize;

    loop {
        if !is_kernel_text_address(pc) {
            break;
        }

        print_trace_address(pc);

        // Unwind stack frame
        let frame = unsafe { (fp as *const StackFrame).sub(1).as_ref() }.unwrap();
        fp = frame.fp;
        pc = frame.ra;
    }
}

/// Returns whether an address lies withing the kernel's `.text` section.
fn is_kernel_text_address(pc: usize) -> bool {
    extern "C" {
        static _stext: usize;
        static _etext: usize;
    }

    unsafe { pc >= (&_stext as *const _ as usize) && pc <= (&_etext as *const _ as usize) }
}

/// Traces the function to which PC belongs and displays both its name and the offset within.
fn print_trace_address(pc: usize) {
    kprint!(" [<{:016x}>] ", pc);
    if let Some((sym, off)) = ksyms::resolve_symbol(pc) {
        kprintln!("<{}>+0x{:x}", sym, off);
    } else {
        kprintln!("?");
    }
}
