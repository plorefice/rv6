//! RISC-V specific entry point.

use fdt::Fdt;

use crate::{
    arch::riscv::earlycon,
    mm::addr::{MemoryAddress, VirtAddr},
};

use super::{mm, sbi, time, trap};

/// Architecture-specific entry point.
///
/// This function performs any RISC-V-specific setup before handing control to the kernel.
///
/// # Safety
///
/// Physical and virtual memory setup is performed here among other things, so a lot of stuff
/// can go wrong.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arch_init(fdt_data: *const u8, kernel_rpt_va: usize, hart_id: usize) {
    // Parse the FDT
    // SAFETY: `fdt_data` is a valid pointer to a valid FDT
    let fdt = unsafe { Fdt::from_raw_ptr(fdt_data) }.unwrap();

    // Initialize core subsystems
    // Order is important here!
    // 1. earlycon to have some logging facilities as early as possible
    // 2. SBI will print out some information to earlycon
    // 3. finish setting up virtual memory, as the rest of the code needs this
    // 4. finish setting up trap frame and handling and enable interrupts
    //
    // Interrupts must remain disabled until tp and kernel stack are valid!
    earlycon::register();
    sbi::show_info();
    mm::setup_late(&fdt, VirtAddr::new(kernel_rpt_va));
    trap::init(hart_id);

    kprintln!("Hart {} initialized", hart_id);
}
