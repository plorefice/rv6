//! RISC-V implementation of process management.

use crate::{
    arch::riscv::{
        instructions::fence_i,
        mm::elf::{RiscvAddrSpace, RiscvLoader},
        mmu,
        registers::{Sepc, Sscratch, Sstatus, SstatusFlags},
    },
    mm::addr::{MemoryAddress, PhysAddr, VirtAddr},
    proc::{ProcessBuilder, ProcessMemoryLayout, StackSpec, UserProcessExecutor},
};

/// RISC-V implementation of the UserProcessExecutor trait.
pub struct RiscvUserProcessExecutor;

impl UserProcessExecutor for RiscvUserProcessExecutor {
    type AddrSpace = RiscvAddrSpace;

    unsafe fn enter_user(&self, aspace: &Self::AddrSpace, entry: VirtAddr, sp: VirtAddr) -> ! {
        // // Set the supervisor trapframe to point to the process's trap frame
        // crate::arch::riscv::stackframe::set_trapframe_pointer(pcb.trap_frame as *mut _);

        // Swap page tables
        // SAFETY: assuming `pcb` has been properly init'd and `rpt_pa` is a valid page address.
        unsafe {
            mmu::switch_page_table(aspace.root_page_table_pa());
        }

        // Configure s-registers for user mode switch
        // SAFETY: assuming memory has been properly mapped and loaded
        unsafe {
            // Prepare user PC and SP
            Sepc::write(entry.as_usize() as u64);
            Sscratch::write(sp.as_usize() as u64);

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

    unsafe fn resume_user(&self, aspace: &Self::AddrSpace) -> ! {
        todo!()
    }
}

/// RISC-V implementation of the ProcessMemoryLayout trait.
pub struct RiscvProcessMemoryLayout;

impl ProcessMemoryLayout for RiscvProcessMemoryLayout {
    fn user_end(&self) -> VirtAddr {
        VirtAddr::new(0x0000_003f_ffff_f000)
    }

    fn default_stack(&self) -> StackSpec {
        let end = self.user_end();
        let size = 8 * 1024 * 1024; // 8 MiB
        let start = end - size;

        StackSpec {
            start,
            end,
            initial_sp: end,
        }
    }
}

pub struct RiscvProcessBuilder {
    loader: RiscvLoader,
    executor: RiscvUserProcessExecutor,
    memory_layout: RiscvProcessMemoryLayout,
}

impl ProcessBuilder for RiscvProcessBuilder {
    type AddrSpace = RiscvAddrSpace;
    type Loader = RiscvLoader;
    type Executor = RiscvUserProcessExecutor;
    type MemoryLayout = RiscvProcessMemoryLayout;

    fn loader(&self) -> &Self::Loader {
        &self.loader
    }

    fn executor(&self) -> &Self::Executor {
        &self.executor
    }

    fn memory_layout(&self) -> &Self::MemoryLayout {
        &self.memory_layout
    }
}

pub fn process_builder() -> impl ProcessBuilder {
    RiscvProcessBuilder {
        loader: RiscvLoader,
        executor: RiscvUserProcessExecutor,
        memory_layout: RiscvProcessMemoryLayout,
    }
}
