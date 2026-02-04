use crate::{
    arch::{
        PAGE_SIZE, RiscvLoader, phys_to_virt,
        riscv::{
            addr::PhysAddr,
            instructions::{fence_i, sfence_vma},
            mmu,
            registers::{Satp, Sepc, Sscratch, Sstatus, SstatusFlags},
        },
    },
    proc::{Process, ProcessMemory},
};

/// Switches running process to the one specified.
///
/// # Safety
/// - `rpt_pa` must be the physical address of a valid root page table containing the process's memory mappings.
pub unsafe fn switch_to_process(rpt_pa: u64, entry: usize, stack_top: usize) -> ! {
    kprintln!("Switching to userspace...");

    // // Set the supervisor trapframe to point to the process's trap frame
    // crate::arch::riscv::stackframe::set_trapframe_pointer(pcb.trap_frame as *mut _);

    // Swap page tables
    // SAFETY: assuming `pcb` has been properly init'd and `rpt_pa` is a valid page address.
    unsafe {
        mmu::switch_page_table(PhysAddr::new(rpt_pa));
    }

    // Configure s-registers for user mode switch
    // SAFETY: assuming memory has been properly mapped and loaded
    unsafe {
        // Prepare user PC and SP
        Sepc::write(entry as u64);
        Sscratch::write(stack_top as u64);

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
