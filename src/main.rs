#![no_std]
#![no_main]
#![feature(asm)]

mod macros;
mod mmio;

use core::panic::PanicInfo;

const UART_BASE: usize = 0x1000_0000;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    loop {
        let lsr = unsafe { mmio::read_u8(UART_BASE, 5) };

        if lsr & 0x1 != 0 {
            unsafe {
                let b = mmio::read_u8(UART_BASE, 0);

                mmio::write_u8(UART_BASE, 0, b);

                if b == b'\r' {
                    mmio::write_u8(UART_BASE, 0, b'\n');
                }
            }
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
