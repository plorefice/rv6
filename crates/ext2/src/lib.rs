#![cfg_attr(not(feature = "std"), no_std)]

mod blocks;
mod error;
mod fs;
mod inode;
mod io;

pub use crate::fs::*;
