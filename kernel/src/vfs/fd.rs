//! File descriptor handling.

use core::io::SeekFrom;

use alloc::{sync::Arc, vec::Vec};
use bitflags::bitflags;
use uapi::Errno;

use crate::{console, sync::SpinLock, vfs::file_ops::FileOps};

/// A file descriptor, which is an index into a process's file descriptor table.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fd(usize);

impl From<usize> for Fd {
    fn from(fd: usize) -> Self {
        Fd(fd)
    }
}

impl From<Fd> for usize {
    fn from(fd: Fd) -> Self {
        fd.0
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

    /// Allocates a new file descriptor for the given `OpenFile` and returns it.
    pub fn alloc(&mut self, file: Arc<OpenFile>) -> Result<Fd, Errno> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(file);
                return Ok(Fd(i));
            }
        }
        Err(Errno::MFile)
    }

    /// Closes the file descriptor `fd`, removing it from the table.
    pub fn close(&mut self, fd: Fd) -> Result<(), Errno> {
        if let Some(slot) = self.slots.get_mut(fd.0) {
            let old = slot.take();
            if old.is_none() {
                return Err(Errno::BadF);
            }
            Ok(())
        } else {
            Err(Errno::BadF)
        }
    }
}

/// An open file, which is a reference-counted wrapper around a file descriptor.
pub struct OpenFile {
    offset: SpinLock<u64>,
    flags: OpenFlags,
    inner: Arc<dyn FileOps>,
}

impl OpenFile {
    /// Creates a new `OpenFile` with the given flags and inner file operations.
    pub fn new(inner: Arc<dyn FileOps>, flags: OpenFlags) -> Self {
        Self {
            offset: SpinLock::new(0),
            flags,
            inner,
        }
    }

    /// Creates a new `OpenFile` for the console device.
    pub fn console() -> Self {
        Self {
            offset: SpinLock::new(0),
            flags: OpenFlags::READ | OpenFlags::WRITE,
            inner: console::get(),
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
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, Errno> {
        if !self.flags.contains(OpenFlags::READ) {
            return Err(Errno::BadF);
        }

        self.inner.read(&self.offset, buf)
    }

    /// Reads the entire contents of the open file into the provided vector.
    ///
    /// Returns the total number of bytes read, or an `Errno` if the read operation fails.
    pub fn read_to_end(&self, buf: &mut Vec<u8>) -> Result<usize, Errno> {
        let mut total_bytes_read = 0;
        let mut temp_buf = [0u8; 4096];

        loop {
            let bytes_read = self.read(&mut temp_buf)?;
            if bytes_read == 0 {
                break;
            }
            buf.extend_from_slice(&temp_buf[..bytes_read]);
            total_bytes_read += bytes_read;
        }

        Ok(total_bytes_read)
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

impl From<uapi::OpenFlags> for OpenFlags {
    fn from(flags: uapi::OpenFlags) -> Self {
        let mut open_flags = OpenFlags::empty();
        if flags.contains(uapi::OpenFlags::O_READ) {
            open_flags |= OpenFlags::READ;
        }
        if flags.contains(uapi::OpenFlags::O_WRITE) {
            open_flags |= OpenFlags::WRITE;
        }
        open_flags
    }
}
