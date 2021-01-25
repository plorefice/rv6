use core::panic::PanicInfo;

use crate::arch::halt;

/// Implements the kernel's panic behavior.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("Kernel panic: {}", info);

    kprintln!("Halting!");

    halt();
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
