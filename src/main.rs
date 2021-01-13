#![no_std]
#![no_main]
#![feature(asm)]

mod macros;
mod mmio;

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    for b in b"Hello world!\r\n" {
        unsafe { mmio::write_u8(0x10000000, 0, *b) };
    }

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
