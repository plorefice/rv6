use super::{sbi, time, trap};

#[no_mangle]
pub extern "C" fn arch_init() {
    println!();

    // Initialize core
    sbi::init();
    trap::init();

    // Start ticking
    time::schedule_next_tick(time::CLINT_TIMEBASE);
}
