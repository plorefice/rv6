//! RISC-V implementation of process management.

use core::mem::MaybeUninit;

use crate::{
    arch::riscv::{
        addr::PhysAddrExt,
        instructions::fence_i,
        mm::{
            GFA, PROC_KSTACK_MEM_OFFSET, PROC_KSTACK_MEM_SIZE,
            elf::{RiscvAddrSpace, RiscvLoader},
        },
        mmu::{self, EntryFlags, PAGE_SIZE},
        registers::{Satp, Sepc, Sstatus, SstatusFlags},
        trap::TrapFrame,
    },
    mm::addr::{MemoryAddress, PhysAddr, VirtAddr},
    proc::{
        Process, ProcessBuilder, ProcessId, ProcessMemoryLayout, ProcessStackLayout, ProcessState,
        StackSpec, UserProcessExecutor,
        elf::{ElfLoadError, ElfLoader, SegmentFlags},
        global_process_table, sched,
    },
};

/// RISC-V implementation of the UserProcessExecutor trait.
pub struct RiscvUserProcessExecutor;

impl UserProcessExecutor for RiscvUserProcessExecutor {
    type AddrSpace = RiscvAddrSpace;

    unsafe fn enter_user(
        &self,
        proc: Process,
        entry: VirtAddr,
        stack_layout: ProcessStackLayout,
    ) -> ! {
        // Swap page tables
        // SAFETY: assuming `pcb` has been properly init'd and `rpt_pa` is a valid page address.
        unsafe {
            mmu::switch_page_table(proc.aspace.root_page_table_pa());
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

        let pid = sched::allocate_process(proc);
        sched::enqueue_process(pid);

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
}

/// Resumes execution of the specified process by switching address spaces and restoring its trap frame.
pub fn resume_process(pid: ProcessId) -> ! {
    let (satp, tf, ti, ksp) = {
        let table = global_process_table().lock();
        let proc = table.get(pid).expect("scheduled process not found");
        assert!(matches!(proc.state, ProcessState::Running));
        let tf = core::ptr::from_ref(&proc.astate.tf);
        let rpt_pa = proc.aspace.root_page_table_pa();
        let satp = (Satp::read_raw() & !0xfff_ffff_ffff_u64) | rpt_pa.page_index() as u64;
        let ti = PROC_KSTACK_MEM_OFFSET.as_usize();
        let ksp = (PROC_KSTACK_MEM_OFFSET + PROC_KSTACK_MEM_SIZE).as_usize();
        (satp, tf, ti, ksp)
    };

    // SAFETY: `tf` points to a valid trap frame in the process table, and `satp`/`ti`/`ksp`
    // describe a valid address space and kernel stack for the target process.
    unsafe {
        resume_context(satp as usize, tf, ti, ksp);
    }
}

unsafe extern "C" {
    fn resume_context(satp: usize, tf: *const TrapFrame, ti: usize, ksp: usize) -> !;
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
        aspace: &mut RiscvAddrSpace,
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

    fn fork(&self, parent: &Process) -> Process {
        let mut aspace = match self.loader().new_user_addr_space() {
            Ok(aspace) => aspace,
            Err(e) => {
                panic!("failed to create user address space: {:?}", e);
            }
        };

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

        // SAFETY: We are the only ones accessing the parent process at this point,
        //         and we are duplicating its address space.
        unsafe {
            let mut gfa = GFA.lock();
            let gfa = gfa.as_mut().expect("GFA not initialized");

            aspace
                .page_table_walker()
                .clone_user_mappings(parent.aspace.page_table(), gfa);
        }

        let mut astate = ProcState {
            ti: ThreadInfo {
                ksp: layout.kernel_stack.initial_sp.as_usize(),
                usp: parent.astate.ti.usp,
            },
            tf: parent.astate.tf.clone(),
        };

        // Child returns 0 from fork and resumes after the ecall instruction.
        astate.tf.a0 = 0;
        astate.tf.epc += 4;

        Process {
            state: ProcessState::Running,
            aspace,
            astate,
            parent: None, // Parent will be set by the caller, we don't have access to the parent's PID here
            children: Default::default(),
        }
    }

    fn destroy(&self, mut process: Process) {
        let rpt_pa = process.aspace.root_page_table_pa();
        let proc_kstack_start = PROC_KSTACK_MEM_OFFSET;
        let proc_kstack_end = PROC_KSTACK_MEM_OFFSET + PROC_KSTACK_MEM_SIZE;

        // SAFETY: the process is no longer runnable and `satp` points at the parent's tables.
        unsafe {
            let mut gfa = GFA.lock();
            let gfa = gfa.as_mut().expect("GFA not initialized");

            process
                .aspace
                .page_table_walker()
                .destroy_aspace(rpt_pa, proc_kstack_start, proc_kstack_end, gfa)
                .expect("failed to destroy process address space");
        }
    }
}

pub fn process_loader() -> impl ElfLoader {
    RiscvLoader
}

pub fn process_builder() -> impl ProcessBuilder {
    RiscvProcessBuilder {
        loader: RiscvLoader,
        executor: RiscvUserProcessExecutor,
        memory_layout: RiscvProcessMemoryLayout,
    }
}

/// A structure containing information required by the core for task handling.
#[derive(Clone)]
#[repr(C)]
pub struct ThreadInfo {
    /// Pointer to the top of the kernel stack.
    pub ksp: usize,
    /// Pointer to the top of the user stack.
    pub usp: usize,
}

pub struct ProcState {
    pub ti: ThreadInfo,
    pub tf: TrapFrame,
}

impl Default for ProcState {
    fn default() -> Self {
        Self {
            ti: ThreadInfo { ksp: 0, usp: 0 },
            // SAFETY: TrapFrame is a plain data structure with no uninitialized fields,
            //         so it's safe to create an uninitialized instance.
            tf: unsafe { MaybeUninit::zeroed().assume_init() },
        }
    }
}
