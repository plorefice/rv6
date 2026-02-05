//! Abstraction layer between architecture-specific code and the architecture-agnostic subsystems.
//!
//! The usage pattern for this module is as follows:
//! - Architecture-agnostic subsystems (e.g., `mm`, `proc`) define traits for the services they
//!   require from the architecture-specific code.
//! - Architecture-specific code (e.g., in `arch/riscv`) implements the necessary traits and types
//!   for the various subsystems (e.g., page layout, I/O mapping, ELF loading, user execution).
//! - This module provides facilities to access/create these arch-specific types.

pub mod cpu;
pub mod mm;
pub mod proc;
