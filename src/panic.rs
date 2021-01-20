use core::{alloc::Layout, panic::PanicInfo};

use crate::arch;

/// Used by the failure mechanism of the compiler.
#[lang = "eh_personality"]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

/// Implements the kernel's panic behavior.
#[panic_handler]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);

    println!("HALT");

    loop {
        unsafe { arch::halt() };
    }
}

/// Implements the kernel's OOM behavior.
#[lang = "oom"]
#[no_mangle]
pub fn rust_oom(_layout: Layout) -> ! {
    panic!("Kernel memory allocation failed");
}

/// Required to handle panics.
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {
        unsafe { arch::halt() };
    }
}
