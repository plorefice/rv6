//! This crate defines the user-space API for the RV6 operating system.
//!
//! It provides a standardized interface for user programs to interact with the kernel through
//! system calls.

#![no_std]

/// Syscall numbers.
#[repr(usize)]
pub enum Sysno {
    /// Write to a file descriptor.
    Write = 0,
    /// Exit the current process.
    Exit = 1,
    /// Fork the current process.
    Fork = 2,
    /// Wait for a child process to exit.
    Wait = 3,
    /// Adjust the program break (heap size).
    Sbrk = 4,
}

impl From<Sysno> for usize {
    fn from(sysno: Sysno) -> Self {
        sysno as usize
    }
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
    /// Bad file descriptor
    BadF = 9,
    /// No child processes
    Child = 10,
    /// Out of memory
    NoMem = 12,
    /// Invalid argument
    Inval = 22,
    /// Function not implemented
    NoSys = 38,
}

impl From<isize> for Errno {
    fn from(code: isize) -> Self {
        match code {
            9 => Errno::BadF,
            10 => Errno::Child,
            12 => Errno::NoMem,
            22 => Errno::Inval,
            38 => Errno::NoSys,
            _ => Errno::Inval,
        }
    }
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
