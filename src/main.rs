#![no_std]
#![no_main]
#![feature(asm, naked_functions, const_raw_ptr_deref, const_mut_refs)]

#[macro_use]
mod macros;

pub mod cpu;
pub mod drivers;
pub mod lib;

use core::panic::PanicInfo;

pub use lib::*;

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

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    println!("{}", RV6_ASCII_LOGO);

    cpu::init_trap_vector();

    unsafe { asm!("ebreak") };

    panic!("Bye bye!");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    abort()
}

/// Halts execution on this hart.
fn abort() -> ! {
    // TODO: this should also disable interrupts.
    loop {
        wfi();
    }
}

/// Blocks execution until an interrupt is pending.
fn wfi() {
    unsafe {
        asm!("wfi", options(nostack));
    }
}
