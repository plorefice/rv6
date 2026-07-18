//! File descriptor handling.

use core::io::SeekFrom;

use alloc::sync::Arc;
use bitflags::bitflags;
use spin::Mutex;
use uapi::Errno;

use crate::{drivers::earlycon, vfs::file_ops::FileOps};

/// A file descriptor, which is an index into a process's file descriptor table.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fd(usize);

impl From<usize> for Fd {
    fn from(fd: usize) -> Self {
        Fd(fd)
    }
}

const FD_MAX: usize = 32;

/// Per-process file descriptor table.
#[derive(Clone)]
pub struct FdTable {
    slots: [Option<Arc<OpenFile>>; FD_MAX],
}

impl FdTable {
    /// Creates a new, empty file descriptor table.
    pub const fn empty() -> Self {
        Self {
            slots: [const { None }; FD_MAX],
        }
    }

    /// Creates a new file descriptor table with standard input, output, and error (fd 0, 1, 2)
    /// set to the kernel console.
    pub fn with_stdio() -> Self {
        let mut table = Self::empty();
        let con = Arc::new(OpenFile::console());
        table.slots[0] = Some(con.clone());
        table.slots[1] = Some(con.clone());
        table.slots[2] = Some(con);
        table
    }

    /// Retrieves the `OpenFile` associated with the given file descriptor.
    pub fn get(&self, fd: Fd) -> Result<Arc<OpenFile>, Errno> {
        self.slots
            .get(fd.0)
            .and_then(|s| s.as_ref())
            .cloned()
            .ok_or(Errno::BadF)
    }
}

/// An open file, which is a reference-counted wrapper around a file descriptor.
pub struct OpenFile {
    offset: Mutex<u64>,
    flags: OpenFlags,
    inner: Arc<dyn FileOps>,
}

impl OpenFile {
    /// Creates a new `OpenFile` for the console device.
    pub fn console() -> Self {
        Self {
            offset: Mutex::new(0),
            flags: OpenFlags::READ | OpenFlags::WRITE,
            inner: Arc::new(earlycon::get()),
        }
    }

    /// Writes data to the open file from the provided buffer.
    ///
    /// Returns the number of bytes written, or an `Errno` if the write operation fails.
    pub fn write(&self, buf: &[u8]) -> Result<usize, Errno> {
        if !self.flags.contains(OpenFlags::WRITE) {
            return Err(Errno::BadF);
        }

        self.inner.write(&self.offset, buf)
    }

    /// Reads data from the open file into the provided buffer.
    ///
    /// Returns the number of bytes read, or an `Errno` if the read operation fails.
    pub fn read(&self, _buf: &mut [u8]) -> Result<usize, Errno> {
        if !self.flags.contains(OpenFlags::READ) {
            return Err(Errno::BadF);
        }

        self.inner.read(&self.offset, _buf)
    }

    /// Seeks to a new position in the open file based on the given offset and seek mode.
    ///
    /// Returns the new position in the file, or an `Errno` if the seek operation fails.
    pub fn seek(&self, whence: SeekFrom) -> Result<u64, Errno> {
        self.inner.seek(&self.offset, whence)
    }
}

bitflags! {
    /// Flags for opening a file.
    pub struct OpenFlags: u32 {
        /// The file has read access.
        const READ = 0b0001;
        /// The file has write access.
        const WRITE = 0b0010;
    }
}
