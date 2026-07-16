#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use rt::{println, proc};
use runtime::{self as rt, proc::Fork};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> isize {
    println!("Hello Rust user space!");

    match unsafe { proc::fork() } {
        Ok(Fork::Child) => {
            println!("Hello from the child process!");
            proc::exit(42);
        }
        Ok(Fork::Parent(pid)) => {
            println!("Hello from the parent process! Child PID: {}", pid);
        }
        Err(e) => {
            println!("Fork failed: {e}");
            proc::exit(-1);
        }
    };

    println!("This line should only be printed once.");

    match proc::wait() {
        Ok(42) => println!("Child process exited with code 42."),
        Ok(code) => println!("Child process exited with a different code: {code}"),
        Err(e) => {
            println!("Wait failed: {e}");
            proc::exit(-1);
        }
    }

    if let Ok(code) = proc::wait() {
        println!("Unexpected child with exit code: {code}");
        proc::exit(-1);
    }

    let mut v = Vec::<u8>::with_capacity(1024);
    v.extend_from_slice(b"Hello from the heap!");
    println!("{}", core::str::from_utf8(&v).unwrap());

    proc::exit(0)
}
