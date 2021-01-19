#![no_std]
#![no_main]
#![feature(asm, naked_functions)]
#![feature(lang_items)]
#![feature(
    const_raw_ptr_deref,
    const_mut_refs,
    const_fn_fn_ptr_basics,
    const_raw_ptr_to_usize_cast
)]

#[macro_use]
mod macros;

pub mod arch;
pub mod drivers;
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

#[no_mangle]
pub extern "C" fn main() -> ! {
    println!("{}", RV6_ASCII_LOGO);

    loop {
        unsafe { arch::halt() };
    }
}
