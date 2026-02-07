#![no_std]

pub mod io;

mod syscall;

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(include_str!("arch/riscv64/start.S"));

#[unsafe(no_mangle)]
pub extern "C" fn __entry(_argc: usize, _argv: *const *const u8, _envp: *const *const u8) -> isize {
    unsafe extern "C" {
        fn main() -> isize;
    }
    unsafe { main() }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
