#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused)] // TODO: remove this when the crate is fully implemented

mod blocks;
mod error;
mod fs;
mod inode;
mod io;

pub use crate::fs::*;
