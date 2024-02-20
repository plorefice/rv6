//! RISC-V specific entry point.

use super::{memory, sbi, time, trap};

/// Architecture-specific entry point.
///
/// This function performs any RISC-V-specific setup before handing control to the kernel.
///
/// # Safety
///
/// Physical and virtual memory setup is performed here among other things, so a lot of stuff
/// can go wrong.
#[no_mangle]
pub unsafe extern "C" fn arch_init(_dtb_addr: usize) {
    // Initialize core subsystems
    sbi::show_info();
    trap::init();
    // SAFETY: no memory safety expectations here
    unsafe { memory::init() };

    // Start ticking
    time::schedule_next_tick(time::CLINT_TIMEBASE);
}
