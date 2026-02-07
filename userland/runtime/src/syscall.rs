use core::arch::asm;

/// Writes `len` bytes from `buf` to the file descriptor `fd`.
pub(crate) fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    syscall3(Syscall::Write, fd, buf as usize, len)
}

/// Perform a syscall with 3 arguments.
fn syscall3(sc: Syscall, arg0: usize, arg1: usize, arg2: usize) -> isize {
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
) -> isize {
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

    arg0 as isize
}

/// Syscall numbers.
enum Syscall {
    Write = 0,
}

impl From<Syscall> for usize {
    fn from(syscall: Syscall) -> Self {
        syscall as usize
    }
}
