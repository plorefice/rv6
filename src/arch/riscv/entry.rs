use mm::page::PhysicalAddress;

use crate::mm;

use super::{sbi, time, trap};

#[no_mangle]
pub extern "C" fn arch_init() {
    kprintln!();

    // Initialize core
    sbi::init();
    trap::init();

    // Initialize memory
    unsafe {
        // Defined in linker script
        extern "C" {
            /// The starting word of the kernel in memory.
            static __start: usize;
            /// The ending word of the kernel in memory.
            static __end: usize;
        }

        let kernel_mem_start = PhysicalAddress::new(&__start as *const usize as usize);
        let kernel_mem_end = PhysicalAddress::new(&__end as *const usize as usize);

        kprintln!("Kernel memory: {} - {}", kernel_mem_start, kernel_mem_end);

        // TODO: parse DTB to get the memory size
        let phys_mem_end = PhysicalAddress::new(0x8000_0000 + 128 * 1024 * 1024);

        mm::frame::init(kernel_mem_end, (phys_mem_end - kernel_mem_end).into()).unwrap();
    }

    // Start ticking
    time::schedule_next_tick(time::CLINT_TIMEBASE);
}
