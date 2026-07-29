//! Syscalls implementation.

use alloc::{string::String, sync::Arc};
use uapi::{Errno, SysArgs, SysResult};

use crate::{
    arch::hal,
    proc::{self, ProcessBuilder, ProcessId, ProcessState, ProcessTable, global_process_table},
    sched,
    vfs::root_fs,
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

/// Copies `src.len()` bytes from the kernel-space buffer `src` to the user-space buffer `dst`.
///
/// # Safety
/// - `dst` must be a valid user-space pointer and the memory region it points to must be accessible.
///   The caller must ensure these conditions are met.
pub unsafe fn copy_to_user(dst: UserPtr<u8>, src: &[u8]) {
    hal::mm::with_user_access(|| unsafe {
        // SAFETY: TODO: validate user pointer
        let mut p = dst.addr as *mut u8;
        for &b in src.iter() {
            core::ptr::write_volatile(p, b);
            p = p.add(1);
        }
    });
}

/// Copies a null-terminated C string from user space to a kernel buffer.
///
/// Returns the number of bytes copied, excluding the null terminator.
///
/// # Safety
/// - `src` must be a valid user-space pointer to a null-terminated string.
/// - `dst` must be a valid kernel-space buffer with enough space to hold the string.
pub unsafe fn copy_cstr_from_user(dst: &mut [u8], src: UserPtr<u8>) -> Result<usize, Errno> {
    hal::mm::with_user_access(|| unsafe {
        // SAFETY: TODO: validate user pointer
        let mut p = src.addr as *const u8;
        let mut i = 0;
        while i < dst.len() {
            let byte = core::ptr::read_volatile(p);
            dst[i] = byte;
            if byte == 0 {
                return Ok(i);
            }
            p = p.add(1);
            i += 1;
        }
        Err(Errno::Inval)
    })
}

/// Reads `len` bytes from the specified file descriptor into the user-space buffer `buf`.
pub fn sys_read(args: SysArgs) -> SysResult<usize> {
    let fd = args.get(0);
    let buf = UserPtr::<u8>::new(args.get(1));
    let len = args.get(2);

    let kbuf = {
        let mut kbuf = vec![0u8; len];
        let file = proc::with_current_process(|p| p.fds.get(fd.into()))?;
        let bytes_read = file.read(&mut kbuf)?;
        kbuf.truncate(bytes_read);
        kbuf
    };

    // SAFETY: the user pointer has been checked to be valid
    unsafe { copy_to_user(buf, &kbuf) };

    Ok(kbuf.len())
}

/// Writes `len` bytes from the user-space buffer `buf` to the specified file descriptor.
pub fn sys_write(args: SysArgs) -> SysResult<usize> {
    let fd = args.get(0);

    // Read the user-space buffer into a kernel-space buffer
    let kbuf = {
        let buf = UserPtr::<u8>::new(args.get(1));
        let len = args.get(2);
        let mut kbuf = vec![0u8; len];
        // SAFETY: the user pointer has been checked to be valid
        unsafe { copy_from_user(&mut kbuf, buf) };
        kbuf
    };

    let file = proc::with_current_process(|p| p.fds.get(fd.into()))?;
    file.write(&kbuf)
}

/// Terminates the current process with the given exit code.
pub fn sys_exit(args: SysArgs) -> ! {
    let exit_code = args.get(0);
    proc::exit_current(exit_code);
}

/// Creates a new process by duplicating the current process.
pub fn sys_fork(args: SysArgs) -> SysResult<usize> {
    let _flags = args.get(0);

    let (child_id, child_pid) = proc::fork_current_process();
    sched::enqueue_process(child_id);
    Ok(child_pid.as_usize())
}

enum WaitPoll {
    Ready {
        child_pid: ProcessId,
        exit_code: usize,
    },
    Empty,
    Pending,
}

/// Waits for a child process to exit and retrieves its exit code.
pub fn sys_wait(_: SysArgs) -> SysResult<usize> {
    let parent_pid = sched::current_process_id().expect("no current process");

    loop {
        {
            let mut proc_table = global_process_table().lock();
            match poll_wait(&proc_table, parent_pid) {
                WaitPoll::Ready {
                    child_pid,
                    exit_code,
                } => {
                    let child = proc_table.take(child_pid).expect("invalid child PID");
                    hal::proc::builder().destroy(child);

                    let parent = proc_table.get_mut(parent_pid).expect("invalid parent PID");
                    parent.children.retain(|&pid| pid != child_pid);

                    return Ok(exit_code);
                }
                WaitPoll::Empty => return Err(Errno::Child),
                WaitPoll::Pending => {
                    // Arm the current process to be woken when a child exits, then park it.
                    // Do this while holding the process table lock to avoid races.
                    let parent = proc_table.get_mut(parent_pid).expect("invalid parent PID");
                    parent.state = ProcessState::Waiting;
                }
            }
        } // unlock proc_table

        sched::park_armed();
    }
}

fn poll_wait(proc_table: &ProcessTable, parent_pid: ProcessId) -> WaitPoll {
    let parent = proc_table.get(parent_pid).expect("invalid parent PID");

    if parent.children.is_empty() {
        return WaitPoll::Empty; // No children to wait for
    }

    for &child_pid in &parent.children {
        if let Some(child) = proc_table.get(child_pid)
            && let ProcessState::Zombie { exit_code } = child.state
        {
            return WaitPoll::Ready {
                child_pid,
                exit_code,
            };
        }
    }

    WaitPoll::Pending // No zombie children found, but there are still children
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

/// Opens a file at the specified `path` with the given `flags`.
pub fn sys_open(args: SysArgs) -> SysResult<usize> {
    const PATH_MAX: usize = 1024; // Define a maximum path length

    let path = UserPtr::<u8>::new(args.get(0));
    let flags = uapi::OpenFlags::from_bits_truncate(args.get(1));

    // SAFETY: the user pointer has been checked to be valid
    let path = unsafe {
        let mut buf = vec![0u8; PATH_MAX];
        let n = copy_cstr_from_user(&mut buf, path)?;
        buf.truncate(n);
        String::from_utf8(buf).map_err(|_| Errno::Inval)?
    };

    let file = root_fs().open(&path, flags.into())?;
    let fd = proc::with_current_process_mut(|p| p.fds.alloc(Arc::new(file)))?;
    Ok(fd.into())
}

/// Closes the file descriptor `fd`, removing it from the process's file descriptor table.
pub fn sys_close(args: SysArgs) -> SysResult<usize> {
    let fd = args.get(0);
    proc::with_current_process_mut(|p| p.fds.close(fd.into()))?;
    Ok(0)
}
