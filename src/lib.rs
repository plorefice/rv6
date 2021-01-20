//! rv6 is an educational, Unix-like kernel inspired by
//! [xv6](https://pdos.csail.mit.edu/6.828/2020/xv6.html), with a focus on the RISC-V architecture.
//! The goal is to achieve a minimal operating system able to boot a userpace init program.
//!
//! rv6 is developed and tested using [QEMU](https://www.qemu.org/). It has not been tested on real
//! hardware and some things may thus not work as expected.

#![no_std]
#![feature(asm, naked_functions)]
#![feature(lang_items)]
#![feature(
    const_raw_ptr_deref,
    const_mut_refs,
    const_fn_fn_ptr_basics,
    const_raw_ptr_to_usize_cast
)]
#![deny(missing_docs)]

#[macro_use]
mod macros;

/// Architecture-specific functions.
pub mod arch;

/// Device and peripheral drivers.
pub mod drivers;

/// Memory management facilities.
pub mod mm;

/// Panic support.
#[cfg(not(any(feature = "doc", test)))]
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
    println!("{}", RV6_ASCII_LOGO);

    loop {
        unsafe { arch::halt() };
    }
}
