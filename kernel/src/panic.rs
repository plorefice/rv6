use core::{alloc::Layout, panic::PanicInfo};

/// Implements the kernel's panic behavior.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("Kernel panic: {}", info);

    kprintln!("Halting!");

    crate::arch::halt();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use crate::drivers::syscon::SYSCON;

    kprintln!("FAILED");
    kprintln!();
    kprintln!("Error: {}", info);

    // Exit from QEMU with error
    SYSCON.lock().poweroff(1);

    unreachable!();
}

#[alloc_error_handler]
fn oom(layout: Layout) -> ! {
    kprintln!("memory allocation of {} bytes failed", layout.size());

    crate::arch::halt();
}
