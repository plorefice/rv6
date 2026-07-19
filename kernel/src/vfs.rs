//! Virtual File System (VFS) module for the rv6 kernel.
//!
//! This module provides an abstraction layer over different file system implementations,
//! allowing the kernel to interact with files and directories in a uniform way, regardless of the
//! underlying file system type.

use spin::Once;

pub mod ext2;
pub mod fd;
pub mod file_ops;

static ROOT_FS: Once<ext2::Fs> = Once::new();

/// Initializes the root file system with the given EXT2 file system instance.
pub fn init_root_fs(fs: ext2::Fs) -> &'static ext2::Fs {
    ROOT_FS.call_once(|| fs)
}

/// Returns a guard that provides exclusive access to the root file system.
pub fn root_fs() -> &'static ext2::Fs {
    ROOT_FS.get().expect("Root FS is not initialized")
}
