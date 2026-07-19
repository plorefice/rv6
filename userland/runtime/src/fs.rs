use core::io;

use alloc::ffi::CString;
use uapi::OpenFlags;

use crate::{
    io::{OwnedFd, Read},
    syscall::{sys_close, sys_open},
};

pub struct File {
    fd: OwnedFd,
}

impl File {
    pub fn open(path: &str) -> io::Result<Self> {
        OpenOptions::new().read(true).open(path)
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use crate::syscall::sys_read;
        let n = sys_read(self.fd.0, buf.as_mut_ptr(), buf.len())?;
        Ok(n)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        // SAFETY: `fd` is owned by this `File` instance, so it's safe to close it when the `File`
        //          is dropped.
        let _ = unsafe { sys_close(self.fd.0) };
    }
}

pub struct OpenOptions {
    flags: OpenFlags,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOptions {
    pub const fn new() -> Self {
        OpenOptions {
            flags: OpenFlags::empty(),
        }
    }

    pub fn read(mut self, read: bool) -> Self {
        self.flags.set(OpenFlags::O_READ, read);
        self
    }

    pub fn write(mut self, write: bool) -> Self {
        self.flags.set(OpenFlags::O_WRITE, write);
        self
    }

    pub fn open(self, path: &str) -> io::Result<File> {
        let c_path = CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null byte"))?;
        let fd = sys_open(c_path.as_ptr(), self.flags)?;
        Ok(File { fd: OwnedFd(fd) })
    }
}
