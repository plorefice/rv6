//! File operations.
//!
//! [`FileOps`] methods run in process context only: the file offset is a sleeping
//! [`Mutex`](crate::sync::Mutex), so callers must have a `current` process.

use core::io::SeekFrom;

use uapi::Errno;

use crate::sync::Mutex;

/// The operations that can be performed on a file in the virtual file system (VFS).
///
/// Implementations must only be invoked from process context (never from IRQ handlers),
/// because the file offset is protected by a sleeping [`Mutex`].
pub trait FileOps: Send + Sync {
    /// Reads data from the file into the provided buffer, starting at the given offset.
    ///
    /// Returns the number of bytes read, or an `Errno` if the read operation fails.
    fn read(&self, off: &Mutex<u64>, buf: &mut [u8]) -> Result<usize, Errno>;

    /// Writes data to the file from the provided buffer, starting at the given offset.
    ///
    /// Returns the number of bytes written, or an `Errno` if the write operation fails.
    fn write(&self, off: &Mutex<u64>, buf: &[u8]) -> Result<usize, Errno>;

    /// Seeks to a new position in the file based on the given offset and seek mode.
    ///
    /// Returns the new position in the file, or an `Errno` if the seek operation fails.
    fn seek(&self, off: &Mutex<u64>, whence: SeekFrom) -> Result<u64, Errno>;
}
