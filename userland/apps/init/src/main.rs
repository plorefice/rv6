#![no_std]
#![no_main]

use rt::{println, proc};
use runtime::{self as rt};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> isize {
    println!("Hello Rust user space!");

    let pid = unsafe { proc::fork() };
    if pid < 0 {
        println!("Fork failed");
    } else if pid == 0 {
        // Child process
        println!("Hello from the child process!");
        proc::exit(42);
    } else {
        // Parent process
        println!("Hello from the parent process!");
    }

    println!("This line should only be printed once.");

    let exit_code = proc::wait();
    if exit_code == 42 {
        println!("Child process exited with code 42.");
    } else {
        println!("Child process exited with a different code.");
    }

    let exit_code = proc::wait();
    if exit_code != -10 {
        println!("Unexpectedly received a second exit code.");
    }

    proc::exit(0)
}
