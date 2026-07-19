#![no_std]
#![no_main]

extern crate alloc;

use core::str;

use alloc::vec::Vec;
use rt::{print, println, proc};
use runtime::{self as rt, fs::File, io::Read, proc::Fork};

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

    let mut f = File::open("/hello.txt").expect("Failed to open /hello.txt");
    let mut buf = Vec::new();
    let n = f.read_to_end(&mut buf).expect("Failed to read /hello.txt");
    let contents = str::from_utf8(&buf[..n]).expect("Failed to convert bytes to string");
    println!("Read from /hello.txt:");
    print!("{}", contents);
    drop(f);

    proc::exit(0)
}
