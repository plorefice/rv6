//! This crate provides RISC-V specific functions and data structures,
//! and access to various system registers.
//!
//! # Features
//!
//!  - `sv39`: use Sv39 MMU specification

#![no_std]
#![warn(missing_docs)]
#![deny(missing_debug_implementations)]
#![feature(asm)]

pub mod addr;
pub mod instructions;
pub mod registers;

pub use addr::{InvalidAddrError, PhysAddr, VirtAddr};
