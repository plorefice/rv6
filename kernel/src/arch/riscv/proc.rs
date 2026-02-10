//! RISC-V implementation of process management.

use crate::{
    arch::riscv::{
        instructions::fence_i,
        mm::{
            PROC_KSTACK_MEM_OFFSET, PROC_KSTACK_MEM_SIZE,
            elf::{RiscvAddrSpace, RiscvLoader},
        },
        mmu::{self, EntryFlags, PAGE_SIZE},
        registers::{Sepc, Sscratch, Sstatus, SstatusFlags},
        trap::TrapFrame,
    },
    mm::addr::{MemoryAddress, PhysAddr, VirtAddr},
    proc::{
        ProcessBuilder, ProcessMemoryLayout, ProcessStackLayout, StackSpec, UserProcessExecutor,
        elf::{ElfLoadError, ElfLoader, SegmentFlags},
    },
};

/// RISC-V implementation of the UserProcessExecutor trait.
pub struct RiscvUserProcessExecutor;

impl UserProcessExecutor for RiscvUserProcessExecutor {
    type AddrSpace = RiscvAddrSpace;

    unsafe fn enter_user(
        &self,
        aspace: &Self::AddrSpace,
        entry: VirtAddr,
        stack_layout: ProcessStackLayout,
    ) -> ! {
        // Swap page tables
        // SAFETY: assuming `pcb` has been properly init'd and `rpt_pa` is a valid page address.
        unsafe {
            mmu::switch_page_table(aspace.root_page_table_pa());
        }

        // Set up thread info struct for this process at the top of the kernel stack,
        // and write its address to TP for use in trap handling.
        let pls = stack_layout.kernel_stack.start.as_mut_ptr::<ThreadInfo>();
        // SAFETY: pls is valid by design, and we are the only ones accessing it at this point.
        unsafe {
            pls.write_volatile(ThreadInfo {
                ksp: stack_layout.kernel_stack.initial_sp.as_usize(),
                usp: stack_layout.user_stack.initial_sp.as_usize(),
            });
        }

        // Configure s-registers for user mode switch
        // SAFETY: assuming memory has been properly mapped and loaded
        unsafe {
            // Prepare user PC
            Sepc::write(entry.as_usize() as u64);

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
                // sp <- usp
                "mv sp, {usp}",
                // sscratch <- thread info
                "csrw sscratch, {ti}",
                // sret to user mode
                "sret",
                usp = in(reg) stack_layout.user_stack.initial_sp.as_usize(),
                ti = in(reg) pls,
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

impl RiscvProcessMemoryLayout {
    fn default_kernel_stack(&self) -> StackSpec {
        let start = PROC_KSTACK_MEM_OFFSET;
        let end = PROC_KSTACK_MEM_OFFSET + PROC_KSTACK_MEM_SIZE;

        StackSpec {
            start,
            end,
            initial_sp: end,
        }
    }

    fn default_user_stack(&self) -> StackSpec {
        let end = VirtAddr::new(0x0000_003f_ffff_f000);
        let size = 8 * 1024 * 1024; // 8 MiB
        let start = end - size;

        StackSpec {
            start,
            end,
            initial_sp: end,
        }
    }
}

impl ProcessMemoryLayout for RiscvProcessMemoryLayout {
    fn default_stack_layout(&self) -> ProcessStackLayout {
        ProcessStackLayout {
            user_stack: self.default_user_stack(),
            kernel_stack: self.default_kernel_stack(),
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

    fn setup_stack_memory(
        &self,
        aspace: &mut Self::AddrSpace,
    ) -> Result<ProcessStackLayout, ElfLoadError> {
        let layout = self.memory_layout().default_stack_layout();

        // Map kernel stack
        self.loader()
            .map_range_alloc(
                aspace.page_table_walker(),
                layout.kernel_stack.start,
                (layout.kernel_stack.end - layout.kernel_stack.start).as_usize(),
                EntryFlags::RW | EntryFlags::ACCESS | EntryFlags::GLOBAL,
            )
            .expect("failed to map kernel stack");

        // Map user stack
        self.loader()
            .map_range_alloc(
                aspace.page_table_walker(),
                layout.user_stack.start,
                (layout.user_stack.end - layout.user_stack.start).as_usize(),
                EntryFlags::RW | EntryFlags::USER | EntryFlags::ACCESS,
            )
            .expect("failed to map user stack");

        Ok(layout)
    }
}

pub fn process_builder() -> impl ProcessBuilder {
    RiscvProcessBuilder {
        loader: RiscvLoader,
        executor: RiscvUserProcessExecutor,
        memory_layout: RiscvProcessMemoryLayout,
    }
}

/// A structure containing information required by the core for task handling.
#[repr(C)]
pub struct ThreadInfo {
    /// Pointer to the top of the kernel stack.
    pub ksp: usize,
    /// Pointer to the top of the user stack.
    pub usp: usize,
}
