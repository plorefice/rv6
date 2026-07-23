//!  EXT2 filesystem integration with the kernel's VFS layer.

use core::io::SeekFrom;

use alloc::sync::Arc;
use ext2::{FileSystem, Inode};
use uapi::Errno;

use crate::{
    block::BlockDevCursor,
    sync::SpinLock,
    vfs::{
        fd::{OpenFile, OpenFlags},
        file_ops::FileOps,
    },
};

/// A wrapper around the EXT2 filesystem.
pub struct Fs {
    fs: Arc<SpinLock<FileSystem<BlockDevCursor>>>,
}

impl Fs {
    /// Creates a new `Fs` instance from the given EXT2 filesystem.
    pub fn new(fs: FileSystem<BlockDevCursor>) -> Self {
        Self {
            fs: Arc::new(SpinLock::new(fs)),
        }
    }

    /// Opens a file at the given path and returns an `OpenFile` instance.
    pub fn open(&self, path: &str, flags: OpenFlags) -> Result<OpenFile, Errno> {
        let fs = self.fs.clone();
        let inode = fs.lock().lookup(path).map_err(ext2_error_to_errno)?;
        if inode.is_dir() {
            return Err(Errno::IsDir);
        }
        Ok(OpenFile::new(Arc::new(Ext2OpenFile { fs, inode }), flags))
    }
}

/// An implementation of the `FileOps` trait for files in the EXT2 filesystem.
pub struct Ext2OpenFile {
    fs: Arc<SpinLock<FileSystem<BlockDevCursor>>>,
    inode: Inode,
}

impl FileOps for Ext2OpenFile {
    fn read(&self, off: &SpinLock<u64>, buf: &mut [u8]) -> Result<usize, Errno> {
        let mut fs = self.fs.lock();
        let offset = *off.lock();
        match fs.read_at(&self.inode, offset, buf) {
            Ok(bytes_read) => {
                *off.lock() += bytes_read as u64;
                Ok(bytes_read)
            }
            Err(_) => Err(Errno::Io),
        }
    }

    fn write(&self, _off: &SpinLock<u64>, _buf: &[u8]) -> Result<usize, Errno> {
        Err(Errno::Inval) // Not implemented yet
    }

    fn seek(&self, _off: &SpinLock<u64>, _whence: SeekFrom) -> Result<u64, Errno> {
        Err(Errno::Inval) // Not implemented yet
    }
}

fn ext2_error_to_errno(err: ext2::Error) -> Errno {
    use ext2::Error::*;

    match err {
        NotFound => Errno::NoEnt,
        NotADirectory => Errno::NotDir,
        IsADirectory => Errno::IsDir,
        InvalidInput | InvalidFilename | BadMagic => Errno::Inval,
        Io(_) | UnexpectedEof | InvalidData => Errno::Io,
        Unsupported => Errno::NoSys,
        _ => Errno::Inval, // Default case for other errors
    }
}
