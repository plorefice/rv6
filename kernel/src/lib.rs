//! rv6 is an educational, Unix-like kernel inspired by
//! [xv6](https://pdos.csail.mit.edu/6.828/2020/xv6.html), with a focus on the RISC-V architecture.
//! The goal is to achieve a minimal operating system able to boot a userpace init program.
//!
//! rv6 is developed and tested using [QEMU](https://www.qemu.org/). It has not been tested on real
//! hardware and some things may thus not work as expected.

// We are building a freestanding binary, so no standard library support for us
#![no_std]
// Keep things clean and tidy
#![warn(missing_docs)]
#![feature(linkage)]

use alloc::{boxed::Box, string::String};

#[macro_use]
extern crate alloc;

/// Utility macros.
#[macro_use]
mod macros;

/// Architecture-specific functions.
pub mod arch;

/// Device and peripheral drivers.
pub mod drivers;

/// Access to kernel symbols for debugging.
pub mod ksyms;

/// Memory management facilities.
pub mod mm;

/// Panic support.
pub mod panic;

const RV6_ASCII_LOGO: &str = r#"
________________________________________/\\\\\_
____________________________________/\\\\////__
 _________________________________/\\\///_______
  __/\\/\\\\\\\___/\\\____/\\\___/\\\\\\\\\\\____
   _\/\\\/////\\\_\//\\\__/\\\___/\\\\///////\\\__
    _\/\\\___\///___\//\\\/\\\___\/\\\______\//\\\_
     _\/\\\___________\//\\\\\____\//\\\______/\\\__
      _\/\\\____________\//\\\______\///\\\\\\\\\/___
       _\///______________\///_________\/////////_____
"#;

/// Kernel entry point after arch-specific initialization.
#[no_mangle]
pub extern "C" fn kmain() -> ! {
    kprintln!("{}", RV6_ASCII_LOGO);

    kprintln!();
    kprintln!("Testing dynamic allocation:");
    kprintln!("  String: {:?}", String::from("Hello kernel! 👋"));
    kprintln!("     Vec: {:?}", vec![1, 2, 45, 12312]);
    kprintln!("     Box: {:?}", Box::new(Some(42)));
    kprintln!();

    #[allow(clippy::empty_loop)]
    loop {}
}
