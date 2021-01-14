#![no_std]
#![no_main]
#![feature(asm)]

mod drivers;
mod lib;
mod macros;

use core::panic::PanicInfo;

use drivers::ns16550::Ns16550;
pub use lib::*;

const UART_BASE: usize = 0x1000_0000;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let mut uart = Ns16550::new(UART_BASE);

    loop {
        if let Some(c) = uart.get() {
            uart.put(c);
        }
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
