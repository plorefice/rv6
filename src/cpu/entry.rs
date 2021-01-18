use super::{sbi, trap};

#[no_mangle]
pub extern "C" fn arch_init() {
    println!();

    sbi::init();
    trap::init();

    sbi::set_timer(10_000_000).unwrap();
}
