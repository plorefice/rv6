use crate::syscall::sys_fork;

pub unsafe fn fork() -> isize {
    sys_fork()
}
