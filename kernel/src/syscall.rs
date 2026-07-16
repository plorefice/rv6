//! Syscalls implementation.

use uapi::{Errno, SysArgs, SysResult};

use crate::{
    arch::hal,
    drivers::earlycon::{self, EarlyCon},
    proc::{self, ProcessBuilder, ProcessId, ProcessState, ProcessTable, global_process_table},
    sched,
};

/// A raw pointer to a user-space memory location.
///
/// User memory cannot be directly dereferenced from kernel space, so this type is used to
/// represent pointers to user memory safely. To access the data pointed to by a `UserPtr`,
/// functions like `copy_from_user` must be used.
#[derive(Debug, Clone, Copy)]
pub struct UserPtr<T> {
    addr: usize,
    _marker: core::marker::PhantomData<T>,
}

impl<T> UserPtr<T> {
    /// Marks a raw user-space pointer.
    pub fn new(addr: usize) -> Self {
        UserPtr {
            addr,
            _marker: core::marker::PhantomData,
        }
    }
}

/// Copies `dst.len()` bytes from the user-space buffer `src` to the kernel-space buffer `dst`.
///
/// # Safety
/// - `src` must be a valid user-space pointer and the memory region it points to must be accessible.
///   The caller must ensure these conditions are met.
pub unsafe fn copy_from_user(dst: &mut [u8], src: UserPtr<u8>) {
    hal::mm::with_user_access(|| unsafe {
        // SAFETY: TODO: validate user pointer
        let mut p = src.addr as *const u8;
        for b in dst.iter_mut() {
            *b = core::ptr::read_volatile(p);
            p = p.add(1);
        }
    });
}

/// Writes `len` bytes from the user-space buffer `buf` to the specified file descriptor.
///
/// # Note
///
/// For simplicity, this implementation only supports writing to `fd=1` (stdout).
pub fn sys_write(args: SysArgs) -> SysResult<usize> {
    let fd = args.get(0);
    let buf = UserPtr::<u8>::new(args.get(1));
    let len = args.get(2);

    // For simplicity, only support fd=1 (stdout)
    if fd != 1 {
        return Err(Errno::Inval);
    }

    // Print each byte to the early console
    hal::mm::with_user_access(|| {
        let mut p = buf.addr as *const u8;
        for _ in 0..len {
            // SAFETY: TODO: validate user pointer
            let byte = unsafe {
                let b = core::ptr::read_volatile(p);
                p = p.add(1);
                b
            };
            earlycon::get().put(byte);
        }
    });

    Ok(len)
}

/// Terminates the current process with the given exit code.
pub fn sys_exit(args: SysArgs) -> ! {
    let exit_code = args.get(0);
    proc::exit_current(exit_code);
}

/// Creates a new process by duplicating the current process.
pub fn sys_fork(args: SysArgs) -> SysResult<usize> {
    let _flags = args.get(0);

    let child_pid = proc::fork_current_process();
    sched::enqueue_process(child_pid);
    Ok(child_pid.pid())
}

/// Waits for a child process to exit and retrieves its exit code.
pub fn sys_wait(_: SysArgs) -> SysResult<usize> {
    let parent_pid = sched::current_process_id().expect("no current process");

    let mut proc_table = global_process_table().lock();

    let (child_pid, exit_code) = find_zombie_child(&proc_table, parent_pid)?;
    let child = proc_table.take(child_pid).expect("invalid child PID");

    hal::proc::builder().destroy(child);

    let parent = proc_table.get_mut(parent_pid).expect("invalid parent PID");
    parent.children.retain(|&pid| pid != child_pid);

    Ok(exit_code)
}

fn find_zombie_child(
    proc_table: &ProcessTable,
    parent_pid: ProcessId,
) -> SysResult<(ProcessId, usize)> {
    let parent = proc_table.get(parent_pid).expect("invalid parent PID");
    for &child_pid in &parent.children {
        if let Some(child) = proc_table.get(child_pid)
            && let ProcessState::Zombie { exit_code } = child.state
        {
            return Ok((child_pid, exit_code));
        }
    }
    Err(Errno::Child)
}

/// Adjusts the program break (heap size) for the current process.
pub fn sys_sbrk(args: SysArgs) -> SysResult<usize> {
    let increment = args.get(0) as isize;

    let mut proc_table = global_process_table().lock();
    let pid = sched::current_process_id().expect("no current process");
    let proc = proc_table.get_mut(pid).expect("invalid current PID");

    let prev_brk = hal::proc::builder()
        .adjust_program_break(proc, increment)
        .map_err(|e| match e {
            proc::BreakError::InvalidIncrement => Errno::Inval,
            proc::BreakError::OutOfMemory => Errno::NoMem,
        })?;

    Ok(prev_brk.as_usize())
}
