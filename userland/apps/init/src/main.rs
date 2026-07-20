#![no_std]
#![no_main]
#![feature(alloc_io)]
#![feature(core_io)]

extern crate alloc;

use runtime::println;

mod cash;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> isize {
    println!("Welcome to the RV6 userland!");
    cash::run()
}
