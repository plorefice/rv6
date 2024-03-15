//! RISC-V specific entry point.

use crate::arch::VirtAddr;

use super::{mm, sbi, time, trap};

/// Architecture-specific entry point.
///
/// This function performs any RISC-V-specific setup before handing control to the kernel.
///
/// # Safety
///
/// Physical and virtual memory setup is performed here among other things, so a lot of stuff
/// can go wrong.
#[no_mangle]
pub unsafe extern "C" fn arch_init(_dtb_pa: usize, kernel_rpt_va: usize) {
    // Initialize core subsystems
    sbi::show_info();
    trap::init();
    mm::setup_late(VirtAddr::new(kernel_rpt_va));

    // Start ticking
    time::schedule_next_tick(time::CLINT_TIMEBASE);
}
