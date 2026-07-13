use crate::syscall::sys_fork;

/// Forks the current process by invoking the `sys_fork` syscall.
///
/// # Safety
///
/// There are many reasons why this function is unsafe. For example, duplicating existing file
/// descriptors can lead to memory safety issues if the file descriptors are not properly managed.
pub unsafe fn fork() -> isize {
    sys_fork()
}
