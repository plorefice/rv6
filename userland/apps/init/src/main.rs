#![no_std]
#![no_main]

use runtime::{self as _, io::Write};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> isize {
    let mut stdout = runtime::io::stdout();
    stdout.write(b"Hello Rust user space!\n").unwrap();

    let pid = unsafe { runtime::proc::fork() };
    if pid < 0 {
        stdout.write(b"Fork failed\n").unwrap();
    } else if pid == 0 {
        // Child process
        stdout.write(b"Hello from the child process!\n").unwrap();
        runtime::proc::exit(42);
    } else {
        // Parent process
        stdout.write(b"Hello from the parent process!\n").unwrap();
    }

    stdout
        .write(b"This line should only be printed once.\n")
        .unwrap();

    let exit_code = runtime::proc::wait();
    stdout
        .write(if exit_code == 42 {
            b"Child process exited with code 42.\n"
        } else {
            b"Child process exited with a different code.\n"
        })
        .unwrap();

    let exit_code = runtime::proc::wait();
    if exit_code != -10 {
        stdout
            .write(b"Unexpectedly received a second exit code.\n")
            .unwrap();
    }

    runtime::proc::exit(0)
}
