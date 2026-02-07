//! Syscalls implementation.

use crate::{
    arch::hal,
    drivers::{
        earlycon::{self, EarlyCon},
        syscon,
    },
};

/// Syscall numbers.
#[repr(usize)]
pub enum Sysno {
    /// Write to a file descriptor.
    Write = 0,
    /// Exit the current process.
    Exit = 1,
}

/// Syscall arguments passed from user space.
#[derive(Debug, Copy, Clone)]
pub struct SysArgs([usize; 6]);

impl SysArgs {
    /// Creates a new `SysArgs` instance from the given array of syscall arguments.
    #[inline]
    pub fn new(args: [usize; 6]) -> Self {
        SysArgs(args)
    }

    /// Retrieves the syscall argument at the specified index.
    #[inline]
    pub fn get(&self, n: usize) -> usize {
        self.0[n]
    }
}

/// Possible syscall error codes.
#[repr(isize)]
pub enum Errno {
    /// Invalid argument
    EINVAL = 22,
    /// Function not implemented
    ENOSYS = 38,
}

/// Syscall result type.
pub type SysResult<T> = Result<T, Errno>;

impl<T> From<Errno> for SysResult<T> {
    fn from(err: Errno) -> Self {
        Err(err)
    }
}

/// Converts a `SysResult` into a raw return value for syscalls.
pub fn to_ret(res: SysResult<usize>) -> usize {
    match res {
        Ok(val) => val,
        Err(err) => (-(err as i64)) as isize as usize, // Return negative error code
    }
}

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
        return Err(Errno::EINVAL);
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
pub fn sys_exit(args: SysArgs) -> SysResult<usize> {
    let _exit_code = args.get(0);

    // We should never reach this point
    kprintln!("Init process terminated unexpectedly, shutting down");
    syscon::poweroff();
    hal::cpu::halt();
}
