//! Virtual File System (VFS) module for the rv6 kernel.
//!
//! This module provides an abstraction layer over different file system implementations,
//! allowing the kernel to interact with files and directories in a uniform way, regardless of the
//! underlying file system type.

pub mod fd;
pub mod file_ops;
