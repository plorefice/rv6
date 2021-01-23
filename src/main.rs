//! rv6 is an educational, Unix-like kernel inspired by
//! [xv6](https://pdos.csail.mit.edu/6.828/2020/xv6.html), with a focus on the RISC-V architecture.
//! The goal is to achieve a minimal operating system able to boot a userpace init program.
//!
//! rv6 is developed and tested using [QEMU](https://www.qemu.org/). It has not been tested on real
//! hardware and some things may thus not work as expected.

// We are building a freestanding binary, so no standard library support for us
#![no_std]
// Don't use the default entry point
#![no_main]
// Keep things clean and tidy
#![deny(missing_docs)]
// Bunch of features required throughout the kernel code
#![feature(asm, naked_functions)]
#![feature(lang_items)]
#![feature(option_expect_none)]
#![feature(
    const_raw_ptr_deref,
    const_mut_refs,
    const_fn_fn_ptr_basics,
    const_raw_ptr_to_usize_cast
)]
// Features required to build a custom test framework
#![feature(custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "run_tests"]

/// Utility macros.
#[macro_use]
mod macros;

/// Architecture-specific functions.
pub mod arch;

/// Device and peripheral drivers.
pub mod drivers;

/// Memory management facilities.
pub mod mm;

/// Panic support.
pub mod panic;

/// Custom test framework.
pub mod testing;

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

    #[cfg(test)]
    {
        use drivers::syscon::SYSCON;

        run_tests();
        SYSCON.lock().poweroff(0);
    }

    #[allow(clippy::empty_loop)]
    loop {}
}
