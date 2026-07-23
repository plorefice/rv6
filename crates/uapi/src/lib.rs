//! This crate defines the user-space API for the RV6 operating system.
//!
//! It provides a standardized interface for user programs to interact with the kernel through
//! system calls.

#![no_std]
#![feature(core_io)]
#![feature(io_error_input_output_error)]
#![feature(io_error_too_many_open_files)]

use core::{error::Error, fmt, io};

use bitflags::bitflags;

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
    /// Open a file.
    Open = 5,
    /// Close a file descriptor.
    Close = 6,
    /// Read from a file descriptor.
    Read = 7,
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
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(isize)]
pub enum Errno {
    /// No such file or directory
    NoEnt = 2,
    /// I/O error
    Io = 5,
    /// Bad file descriptor
    BadF = 9,
    /// No child processes
    Child = 10,
    /// Resource temporarily unavailable
    Again = 11,
    /// Out of memory
    NoMem = 12,
    /// Not a directory
    NotDir = 20,
    /// Is a directory
    IsDir = 21,
    /// Invalid argument
    Inval = 22,
    /// Too many open files
    MFile = 24,
    /// Function not implemented
    NoSys = 38,
}

impl From<isize> for Errno {
    fn from(code: isize) -> Self {
        match code {
            2 => Errno::NoEnt,
            5 => Errno::Io,
            9 => Errno::BadF,
            10 => Errno::Child,
            11 => Errno::Again,
            12 => Errno::NoMem,
            20 => Errno::NotDir,
            21 => Errno::IsDir,
            22 => Errno::Inval,
            24 => Errno::MFile,
            38 => Errno::NoSys,
            _ => Errno::Inval,
        }
    }
}

impl From<Errno> for io::Error {
    fn from(value: Errno) -> Self {
        match value {
            Errno::NoEnt => io::ErrorKind::NotFound,
            Errno::Io => io::ErrorKind::InputOutputError,
            Errno::BadF => io::ErrorKind::InvalidInput,
            Errno::Child => io::ErrorKind::Other,
            Errno::Again => io::ErrorKind::WouldBlock,
            Errno::NoMem => io::ErrorKind::OutOfMemory,
            Errno::NotDir => io::ErrorKind::NotADirectory,
            Errno::IsDir => io::ErrorKind::IsADirectory,
            Errno::Inval => io::ErrorKind::InvalidInput,
            Errno::MFile => io::ErrorKind::TooManyOpenFiles,
            Errno::NoSys => io::ErrorKind::Unsupported,
        }
        .into()
    }
}

impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            Errno::NoEnt => "No such file or directory",
            Errno::Io => "I/O error",
            Errno::BadF => "Bad file descriptor",
            Errno::Child => "No child processes",
            Errno::Again => "Resource temporarily unavailable",
            Errno::NoMem => "Out of memory",
            Errno::NotDir => "Not a directory",
            Errno::IsDir => "Is a directory",
            Errno::Inval => "Invalid argument",
            Errno::MFile => "Too many open files",
            Errno::NoSys => "Function not implemented",
        };
        write!(f, "{}", description)
    }
}

impl Error for Errno {}

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

bitflags! {
    /// Flags for opening files.
    pub struct OpenFlags: usize {
        /// Read permissions
        const O_READ   = 0b0001;
        /// Write permissions
        const O_WRITE  = 0b0010;
    }
}
