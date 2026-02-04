use crate::{
    arch::riscv::{
        instructions::fence_i,
        mmu,
        registers::{Sepc, Sscratch, Sstatus, SstatusFlags},
    },
    mm::addr::{PhysAddr, VirtAddr},
};

/// Switches running process to the one specified.
///
/// # Safety
/// - `rpt_pa` must be the physical address of a valid root page table containing the process's memory mappings.
pub unsafe fn switch_to_process(rpt_pa: PhysAddr, entry: VirtAddr, stack_top: VirtAddr) -> ! {
    kprintln!("Switching to userspace...");

    // // Set the supervisor trapframe to point to the process's trap frame
    // crate::arch::riscv::stackframe::set_trapframe_pointer(pcb.trap_frame as *mut _);

    // Swap page tables
    // SAFETY: assuming `pcb` has been properly init'd and `rpt_pa` is a valid page address.
    unsafe {
        mmu::switch_page_table(rpt_pa);
    }

    // Configure s-registers for user mode switch
    // SAFETY: assuming memory has been properly mapped and loaded
    unsafe {
        // Prepare user PC and SP
        Sepc::write(entry.as_usize() as u64);
        Sscratch::write(stack_top.as_usize() as u64);

        // Prepare switch to U-mode
        Sstatus::update(|f| {
            f.remove(SstatusFlags::SPP); // Set to user mode
            f.insert(SstatusFlags::SPIE); // Enable interrupts on return to user mode
        });
    }

    // Ensure instruction cache is up to date after loading process
    fence_i();

    // Switch to user stack and jump to user mode
    // NOTE: stack swap and sret must be "atomic": no stack usage must happen in between!
    // SAFETY: everything is properly set up for user mode.
    unsafe {
        core::arch::asm!(
            // sp <- user sp, sscratch <- kernel sp
            "csrrw sp, sscratch, sp",
            // sret to user mode
            "sret",
            options(noreturn)
        );
    }
}
