#![no_std]
#![no_main]

use runtime::{self as _, io::Write};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    let mut stdout = runtime::io::stdout();
    stdout.write(b"Hello Rust user space!\n").unwrap();

    let pid = unsafe { runtime::proc::fork() };
    if pid < 0 {
        stdout.write(b"Fork failed\n").unwrap();
    } else if pid == 0 {
        // Child process
        stdout.write(b"Hello from the child process!\n").unwrap();
    } else {
        // Parent process
        stdout.write(b"Hello from the parent process!\n").unwrap();
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
