//! RISC-V specific entry point.

use fdt::Fdt;

use crate::{
    arch::{self, ArchServices, riscv::earlycon},
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
pub unsafe extern "C" fn arch_init(fdt_data: *const u8, kernel_rpt_va: usize) {
    // Parse the FDT
    // SAFETY: `fdt_data` is a valid pointer to a valid FDT
    let fdt = unsafe { Fdt::from_raw_ptr(fdt_data) }.unwrap();

    // Initialize core subsystems
    earlycon::register();
    sbi::show_info();
    trap::init();
    mm::setup_late(&fdt, VirtAddr::new(kernel_rpt_va));

    // Initialize arch services
    arch::set_arch_services(ArchServices {
        page_layout: arch::ArchPageLayout::new(),
        dma: arch::ArchDmaAllocator::new(),
        io: arch::ArchIoMapper::new(),
        loader: arch::ArchLoaderImpl::new(),
        uexec: arch::ArchUserExecutor::new(),
        uml: arch::ArchUserMemoryLayout::new(),
        halt: arch::halt,
    });
}
