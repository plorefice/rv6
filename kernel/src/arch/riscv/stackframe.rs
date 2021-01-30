//! Stack unwinding for RISC-V.
//!
//! **IMPORTANT:** in order for unwinding to work, the kernel must be compiled using `rustc`'s
//! `force-frame-pointers=yes` option.

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
    unsafe { asm!("add {}, fp, zero", out(reg) fp) };

    let mut pc = walk_stack_frame as *const fn() as usize;

    loop {
        if !is_kernel_text_address(pc) {
            break;
        }

        print_trace_address(pc);

        // Unwind stack frame
        let frame = unsafe { (fp as *const StackFrame).sub(1).as_ref() }.unwrap();
        fp = frame.fp;
        pc = trace_ret_addr(frame.ra);
    }
}

/// Returns whether an address lies withing the kernel's `.text` section.
fn is_kernel_text_address(pc: usize) -> bool {
    extern "C" {
        static __text_start: usize;
        static __text_end: usize;
    }

    unsafe {
        pc >= (&__text_start as *const _ as usize) && pc <= (&__text_end as *const _ as usize)
    }
}

fn print_trace_address(pc: usize) {
    kprintln!(" [{:016x}] name_goes_here", pc);
}

fn trace_ret_addr(ra: usize) -> usize {
    // TODO: use `ra` to find the corresponding symbol
    ra
}
