#![no_std]
#![no_main]
#![feature(asm)]

mod macros;

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    println!("Hello world!");

    loop {
        wfi();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println!("{}", _info);
    abort()
}

/// Halts execution on this hart.
fn abort() -> ! {
    // TODO: this should also disable interrupts.
    loop {
        wfi();
    }
}

/// Blocks execution until an interrupt is pending.
fn wfi() {
    unsafe {
        asm!("wfi", options(nostack));
    }
}
