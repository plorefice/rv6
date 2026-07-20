#![no_std]
#![feature(alloc_io)]
#![feature(allocator_api)]
#![feature(core_io)]

extern crate alloc;

pub mod allocator;
pub mod fs;
pub mod io;
pub mod proc;

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
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    io::stdout().write_fmt(format_args!("{}\n", info)).ok();
    proc::exit(-1)
}
