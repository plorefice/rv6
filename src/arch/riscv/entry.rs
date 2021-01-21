use super::{memory, sbi, time, trap};

#[no_mangle]
pub extern "C" fn arch_init() {
    kprintln!();

    // Initialize core subsystems
    sbi::init();
    trap::init();
    memory::init();

    // Start ticking
    time::schedule_next_tick(time::CLINT_TIMEBASE);
}
