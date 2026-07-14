use crate::syscall::{sys_exit, sys_fork, sys_wait};

/// Forks the current process by invoking the `sys_fork` syscall.
///
/// # Safety
///
/// There are many reasons why this function is unsafe. For example, duplicating existing file
/// descriptors can lead to memory safety issues if the file descriptors are not properly managed.
pub unsafe fn fork() -> isize {
    sys_fork()
}

/// Exits the current process by invoking the `sys_exit` syscall.
pub fn exit(exit_code: usize) -> ! {
    sys_exit(exit_code)
}

/// Waits for a child process to exit by invoking the `sys_wait` syscall.
pub fn wait() -> isize {
    sys_wait()
}
