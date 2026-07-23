//! Early console support.

use core::{fmt, io::SeekFrom};

use uapi::Errno;

use crate::{sync::SpinLock, vfs::file_ops::FileOps};

/// The global early console instance.
static EARLY_CONSOLE: spin::Once<&'static dyn EarlyCon> = spin::Once::new();

/// A trait for early console drivers.
///
/// An early console is a minimal console used during the early stages of kernel
/// initialization, before the full console subsystem is available.
pub trait EarlyCon: Send + Sync {
    /// Writes a single byte to the early console.
    fn put(&self, byte: u8);

    /// Writes a buffer of bytes to the early console.
    fn write(&self, buf: &[u8]) {
        for &byte in buf {
            self.put(byte);
        }
    }
}

impl<T: EarlyCon> FileOps for T {
    fn read(&self, _off: &SpinLock<u64>, _buf: &mut [u8]) -> Result<usize, Errno> {
        Err(Errno::Inval)
    }

    fn write(&self, _off: &SpinLock<u64>, buf: &[u8]) -> Result<usize, Errno> {
        self.write(buf);
        Ok(buf.len())
    }

    fn seek(&self, _off: &SpinLock<u64>, _whence: SeekFrom) -> Result<u64, Errno> {
        Err(Errno::Inval)
    }
}

/// A reference to the early console that implements `fmt::Write`.
pub struct EarlyConRef;

impl EarlyCon for EarlyConRef {
    fn put(&self, byte: u8) {
        if let Some(con) = EARLY_CONSOLE.get() {
            con.put(byte);
        }
    }
}

impl fmt::Write for EarlyConRef {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(con) = EARLY_CONSOLE.get() {
            con.write(s.as_bytes());
        }
        Ok(())
    }
}

/// Initializes the global early console.
pub fn register(console: &'static dyn EarlyCon) {
    EARLY_CONSOLE.call_once(|| console);
}

/// Returns a reference for the early console.
///
/// Note that no guarantee is made that an early console has been registered.
/// In such case, the returned reference will be a no-op implementation.
pub fn get() -> EarlyConRef {
    EarlyConRef
}
