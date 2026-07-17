#![cfg_attr(not(feature = "std"), no_std)]
#![feature(core_io)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod blocks;
mod directory;
mod error;
mod file;
mod fs;
mod inode;
mod io;
mod superblock;

pub use error::*;
pub use fs::*;
pub use io::*;
