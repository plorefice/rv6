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
#![warn(clippy::missing_safety_doc)]
#![warn(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(linkage)]

use alloc::{boxed::Box, string::String};
use fdt::Fdt;

use crate::arch::wfi;

#[macro_use]
extern crate alloc;

#[macro_use]
pub mod macros;

pub mod arch;
pub mod config;
pub mod drivers;
pub mod ksyms;
pub mod mm;
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
///
/// # Safety
///
/// This function uses the raw pointer to the FDT passed from the entry code, and as such is
/// unsafe.
#[no_mangle]
pub unsafe extern "C" fn kmain(fdt_data: *const u8) -> ! {
    kprintln!("{}", RV6_ASCII_LOGO);

    kprintln!();
    kprintln!("Testing dynamic allocation:");
    kprintln!("  String: {:?}", String::from("Hello kernel! 👋"));
    kprintln!("     Vec: {:?}", vec![1, 2, 45, 12312]);
    kprintln!("     Box: {:?}", Box::new(Some(42)));
    kprintln!();

    // SAFETY: assuming the caller has provided us with a valid FDT data pointer
    let fdt = unsafe { Fdt::from_raw_ptr(fdt_data) }.expect("invalid fdt data");

    // Subsystem initialization
    drivers::init(&fdt).expect("driver initialization failed");

    loop {
        wfi();
        kprintln!("Main task awoke!");
    }
}
