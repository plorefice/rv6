use core::io;

use crate::syscall::{sys_exit, sys_fork, sys_wait};

/// Represents the result of a fork operation.
pub enum Fork {
    Parent(usize), // PID of the child process
    Child,         // In the child process
}

/// Forks the current process by invoking the `sys_fork` syscall.
///
/// # Safety
///
/// There are many reasons why this function is unsafe. For example, duplicating existing file
/// descriptors can lead to memory safety issues if the file descriptors are not properly managed.
pub unsafe fn fork() -> Result<Fork, io::Error> {
    match sys_fork() {
        Ok(0) => Ok(Fork::Child),
        Ok(pid) => Ok(Fork::Parent(pid)),
        Err(_) => Err(io::Error::from(io::ErrorKind::Other)),
    }
}

/// Exits the current process by invoking the `sys_exit` syscall.
pub fn exit(exit_code: isize) -> ! {
    sys_exit(exit_code)
}

/// Waits for a child process to exit by invoking the `sys_wait` syscall.
pub fn wait() -> Result<usize, io::Error> {
    match sys_wait() {
        Ok(exit_code) => Ok(exit_code),
        Err(_) => Err(io::Error::from(io::ErrorKind::Other)),
    }
}
