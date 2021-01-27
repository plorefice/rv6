//! This crate provides RISC-V specific functions and data structures,
//! and access to various system registers.
//!
//! # Features
//!
//!  - `sv39`: use Sv39 MMU specification
//!  - `sv48`: use Sv48 MMU specification

#![no_std]
#![warn(missing_docs)]
#![deny(missing_debug_implementations)]
#![feature(asm)]
// Require explicit unsafe blocks even in unsafe fn
#![feature(unsafe_block_in_unsafe_fn)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod addr;
pub mod instructions;
pub mod mmu;
pub mod registers;

pub use addr::{InvalidAddrError, PhysAddr, VirtAddr};
