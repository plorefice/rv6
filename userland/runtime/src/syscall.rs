use core::{arch::asm, num::NonZero};

use uapi::{Errno, OpenFlags, SysResult, Sysno};

/// Reads `len` bytes from the file descriptor `fd` into `buf`.
pub(crate) fn sys_read(fd: usize, buf: *mut u8, len: usize) -> SysResult<usize> {
    syscall3(Sysno::Read, fd, buf as usize, len)
}

/// Writes `len` bytes from `buf` to the file descriptor `fd`.
pub(crate) fn sys_write(fd: usize, buf: *const u8, len: usize) -> SysResult<usize> {
    syscall3(Sysno::Write, fd, buf as usize, len)
}

/// Forks the current process, creating a new child process.
pub(crate) fn sys_fork() -> SysResult<usize> {
    syscall0(Sysno::Fork)
}

/// Adjusts the program break (heap size) for the current process.
///
/// # Safety
///
/// This function is unsafe because it can break the memory safety guarantees of the program.
/// The caller must ensure that the program is not using the impacted memory after the call.
pub(crate) unsafe fn sys_sbrk(increment: isize) -> SysResult<NonZero<usize>> {
    let result = syscall1(Sysno::Sbrk, increment as usize)?;
    NonZero::new(result).ok_or(Errno::NoMem)
}

/// Exits the current process with the given exit code.
pub(crate) fn sys_exit(exit_code: isize) -> ! {
    syscall1(Sysno::Exit, exit_code as usize).ok();
    unreachable!("sys_exit should not return");
}

/// Waits for a child process to exit and retrieves its exit code.
pub(crate) fn sys_wait() -> SysResult<usize> {
    syscall0(Sysno::Wait)
}

/// Opens a file at the specified `path` with the given `flags`.
pub(crate) fn sys_open(path: *const u8, flags: OpenFlags) -> SysResult<usize> {
    syscall2(Sysno::Open, path as usize, flags.bits())
}

/// Closes the file descriptor `fd`, removing it from the process's file descriptor table.
///
/// # Safety
///
/// This function is unsafe because it can lead to undefined behavior if the file descriptor
/// is used after being closed. The caller must ensure that the file descriptor is not used
/// after this call.
pub(crate) unsafe fn sys_close(fd: usize) -> SysResult<usize> {
    syscall1(Sysno::Close, fd)
}

fn syscall0(sc: Sysno) -> SysResult<usize> {
    syscall6(sc.into(), 0, 0, 0, 0, 0, 0)
}

fn syscall1(sc: Sysno, arg0: usize) -> SysResult<usize> {
    syscall6(sc.into(), arg0, 0, 0, 0, 0, 0)
}

fn syscall2(sc: Sysno, arg0: usize, arg1: usize) -> SysResult<usize> {
    syscall6(sc.into(), arg0, arg1, 0, 0, 0, 0)
}

fn syscall3(sc: Sysno, arg0: usize, arg1: usize, arg2: usize) -> SysResult<usize> {
    syscall6(sc.into(), arg0, arg1, arg2, 0, 0, 0)
}

/// Perform a syscall with up to 6 arguments.
fn syscall6(
    num: usize,
    mut arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> SysResult<usize> {
    unsafe {
        asm!("ecall",
                inout("a0") arg0,
                in("a1") arg1,
                in("a2") arg2,
                in("a3") arg3,
                in("a4") arg4,
                in("a5") arg5,
                in("a7") num);
    }

    if arg0 as isize >= 0 {
        Ok(arg0)
    } else {
        Err(Errno::from(-(arg0 as i64) as isize))
    }
}
