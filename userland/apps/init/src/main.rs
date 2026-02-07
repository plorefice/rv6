#![no_std]
#![no_main]

use runtime::{self as _, io::Write};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> isize {
    let mut stdout = runtime::io::stdout();
    stdout.write(b"Hello Rust user space!\n").unwrap();
    0
}
