//! RISC-V implementation of process management.

use core::{
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use crate::{
    arch::riscv::{
        addr::PhysAddrExt,
        context,
        instructions::fence_i,
        mm::{
            GFA, PROC_KSTACK_MEM_OFFSET, PROC_KSTACK_MEM_SIZE,
            elf::{RiscvAddrSpace, RiscvLoader},
            kstack_layout,
        },
        mmu::{self, EntryFlags, PAGE_SIZE},
        registers::{Satp, SstatusFlags},
        time,
        trap::TrapFrame,
    },
    mm::addr::{Align, MemoryAddress, PhysAddr, VirtAddr},
    proc::{
        BreakError, Process, ProcessBuilder, ProcessId, ProcessMemoryLayout, ProcessStackLayout,
        ProcessState, StackSpec, UserProcessExecutor,
        elf::{ElfLoadError, ElfLoader, SegmentFlags},
        global_process_table, sched,
    },
};

/// A structure containing information required by the core for task handling.
#[derive(Clone)]
#[repr(C)]
pub struct ThreadInfo {
    /// Pointer to the top of the kernel stack.
    pub ksp: usize,
    /// Pointer to the top of the user stack.
    pub usp: usize,
}

/// A structure representing the saved kernel thread context of a process.
///
/// Layout must match the offsets in `swtch.S`.
#[derive(Clone)]
#[repr(C)]
pub struct Context {
    /// Return address (`ra`).
    pub ra: usize,
    /// Stack pointer (`sp`).
    pub sp: usize,
    /// Callee-saved registers (`s0`–`s11`).
    pub s: [usize; 12],
}

const _: () = {
    assert!(core::mem::offset_of!(Context, ra) == 0);
    assert!(core::mem::offset_of!(Context, sp) == 8);
    assert!(core::mem::offset_of!(Context, s) == 16);
    assert!(core::mem::size_of::<Context>() == 112);
};

/// Architecture-specific process state.
pub struct ProcState {
    /// Kernel/user stack pointers for trap entry.
    pub ti: ThreadInfo,
    /// Saved user trap frame (for `sret` via the `return_to_user` trampoline).
    pub tf: TrapFrame,
    /// Saved kernel context (for [`context::switch_context`]).
    pub ctx: Context,
}

impl Default for ProcState {
    fn default() -> Self {
        Self {
            ti: ThreadInfo { ksp: 0, usp: 0 },
            // SAFETY: TrapFrame is plain data with no uninit niches.
            tf: unsafe { MaybeUninit::zeroed().assume_init() },
            ctx: Context {
                ra: 0,
                sp: 0,
                s: [0; 12],
            },
        }
    }
}

/// Hart SATP value while running the idle/bootstrap context (kernel page tables).
static IDLE_SATP: AtomicUsize = AtomicUsize::new(0);
/// Hart `tp` (kernel [`ThreadInfo`]) for the idle/bootstrap context.
static IDLE_TP: AtomicUsize = AtomicUsize::new(0);
/// Saved callee-saved state for the idle/bootstrap context.
static mut IDLE_CTX: Context = Context {
    ra: 0,
    sp: 0,
    s: [0; 12],
};

/// Records the kernel SATP/`tp` used when switching into the idle context.
pub fn init_idle(hart_id: usize) {
    IDLE_SATP.store(Satp::read_raw() as usize, Ordering::Relaxed);
    IDLE_TP.store(kstack_layout(hart_id).start.as_usize(), Ordering::Relaxed);
}

/// Initial kernel context: first `switch` into this process lands in [`return_to_user`].
fn initial_context(ksp: usize) -> Context {
    Context {
        ra: return_to_user as *const () as usize,
        sp: ksp & !0xf,
        s: [0; 12],
    }
}

/// Build a trap frame that [`resume_process`] can `sret` into userspace with.
fn initial_trapframe(entry: VirtAddr, usp: VirtAddr) -> TrapFrame {
    // SAFETY: TrapFrame is plain data; zero is a valid starting point.
    let mut tf: TrapFrame = unsafe { MaybeUninit::zeroed().assume_init() };
    tf.epc = entry.as_usize();
    tf.sp = usp.as_usize();
    // User mode on sret: SPP=0, SPIE=1 (enable IRQs after sret), SIE=0 in S-mode.
    tf.status = SstatusFlags::SPIE.bits() as usize;
    tf
}

/// RISC-V implementation of the UserProcessExecutor trait.
pub struct RiscvUserProcessExecutor;

impl UserProcessExecutor for RiscvUserProcessExecutor {
    type AddrSpace = RiscvAddrSpace;

    unsafe fn enter_user(
        &self,
        mut proc: Process,
        entry: VirtAddr,
        stack_layout: ProcessStackLayout,
    ) -> ! {
        let ksp = stack_layout.kernel_stack.initial_sp.as_usize();
        let usp = stack_layout.user_stack.initial_sp.as_usize();

        proc.astate.ti = ThreadInfo { ksp, usp };
        proc.astate.tf = initial_trapframe(entry, stack_layout.user_stack.initial_sp);
        proc.astate.ctx = initial_context(ksp);

        let proc_rpt = proc.aspace.root_page_table_pa();
        let kernel_rpt = PhysAddr::from_ppn(Satp::read_ppn() as usize);

        // Write ThreadInfo into the process's private kstack mapping.
        // SAFETY: switching to the new process page tables to initialize its kstack.
        unsafe {
            mmu::switch_page_table(proc_rpt);
            stack_layout
                .kernel_stack
                .start
                .as_mut_ptr::<ThreadInfo>()
                .write_volatile(ThreadInfo { ksp, usp });
            fence_i();
            mmu::switch_page_table(kernel_rpt);
        }

        let pid = sched::allocate_process(proc);
        // Do not make it current here — idle will schedule it via `switch(None, Some(pid))`.
        sched::enqueue_process(pid);

        // Bootstrap: run the idle loop, which switches into `pid` via return_to_user.
        idle_main();
    }
}

/// First (and subsequent trampoline) entry into a process via `switch`: `sret` to userspace.
extern "C" fn return_to_user() -> ! {
    let pid = sched::current_process_id().expect("return_to_user: no current process");
    resume_process(pid);
}

/// Idle loop: schedule a runnable process or `wfi`.
///
/// Entered from [`UserProcessExecutor::enter_user`] on the boot stack. The first
/// [`switch`]`(None, Some(_))` saves that stack into [`IDLE_CTX`]; later
/// [`switch`]`(Some(_), None)` returns here.
extern "C" fn idle_main() -> ! {
    loop {
        let next = sched::take_next();
        match next {
            Some(pid) => {
                {
                    let pt = global_process_table().lock();
                    let proc = pt.get(pid).expect("idle: next process missing");
                    assert!(matches!(proc.state, ProcessState::Running));
                }
                switch(None, Some(pid));
            }
            None => {
                crate::arch::hal::cpu::local_irq_enable();
                crate::arch::hal::cpu::idle();
                crate::arch::hal::cpu::local_irq_disable();
            }
        }
    }
}

/// Preserve SATP mode/ASID; replace only the root PPN.
fn satp_with_root(rpt_pa: PhysAddr) -> usize {
    ((Satp::read_raw() & !0xfff_ffff_ffff_u64) | rpt_pa.page_index() as u64) as usize
}

/// Switch kernel contexts. `None` is the per-hart idle/scheduler context.
///
/// Returns when some other task switches back to `outgoing`.
pub fn switch(outgoing: Option<ProcessId>, next: Option<ProcessId>) {
    if outgoing == next {
        return;
    }

    let (out_ctx, next_ctx, satp, tp) = {
        let mut table = global_process_table().lock();

        let out_ctx = match outgoing {
            Some(pid) => {
                let proc = table.get_mut(pid).expect("outgoing process not found");
                &raw mut proc.astate.ctx
            }
            // SAFETY: forming a raw pointer to the idle `swtch` slot; not dereferenced here.
            None => unsafe { core::ptr::addr_of_mut!(IDLE_CTX) },
        };

        let (next_ctx, satp, tp) = match next {
            Some(pid) => {
                let proc = table.get(pid).expect("next process not found");
                let satp = satp_with_root(proc.aspace.root_page_table_pa());
                (
                    &raw const proc.astate.ctx,
                    satp,
                    PROC_KSTACK_MEM_OFFSET.as_usize(),
                )
            }
            None => {
                let satp = IDLE_SATP.load(Ordering::Relaxed);
                let tp = IDLE_TP.load(Ordering::Relaxed);
                assert!(satp != 0 && tp != 0, "idle context not initialized");
                // SAFETY: forming a raw pointer to the idle `swtch` slot; not dereferenced here.
                (unsafe { core::ptr::addr_of!(IDLE_CTX) }, satp, tp)
            }
        };

        (out_ctx, next_ctx, satp, tp)
    };

    // SAFETY: `out_ctx` and `next_ctx` are distinct context slots; `satp`/`tp` describe `next`.
    unsafe { context::switch_context(out_ctx, next_ctx, satp, tp) };
}

/// Resumes execution of the specified process by switching address spaces and restoring its trap frame.
fn resume_process(pid: ProcessId) -> ! {
    let (satp, tf, ti, ksp) = {
        let table = global_process_table().lock();
        let proc = table.get(pid).expect("scheduled process not found");
        assert!(matches!(proc.state, ProcessState::Running));
        let tf = core::ptr::from_ref(&proc.astate.tf);
        let satp = satp_with_root(proc.aspace.root_page_table_pa());
        let ti = PROC_KSTACK_MEM_OFFSET.as_usize();
        let ksp = proc.astate.ti.ksp;
        (satp, tf, ti, ksp)
    };

    // SAFETY: `tf` points to a valid trap frame in the process table, and `satp`/`ti`/`ksp`
    // describe a valid address space and kernel stack for the target process.
    unsafe {
        resume_context(satp, tf, ti, ksp);
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
        // Top of the Sv39 lower half, with one unmapped page as a guard against
        // the non-canonical hole / kernel half.
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

/// Process builder for RISC-V.
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

    fn adjust_program_break(
        &self,
        proc: &mut Process,
        increment: isize,
    ) -> Result<VirtAddr, BreakError> {
        if increment < 0 {
            if increment == isize::MIN {
                return Err(BreakError::InvalidIncrement);
            }
            return proc.heap.reclaim((-increment) as usize);
        }

        let increment = increment as usize;

        // Check if the current heap has enough available space to accommodate the requested increment.
        // If so, no new pages need to be mapped, and we can simply adjust the program break.
        if let Some(brk) = proc.heap.try_reserve(increment) {
            return Ok(brk);
        }

        let prev_brk = proc.heap.brk();

        // Not enough space -> map new pages to extend the heap.
        let mapped_increment = increment - proc.heap.available_space();
        let mapped_increment = mapped_increment.next_multiple_of(PAGE_SIZE);
        assert!(proc.heap.mapped_end().is_aligned(PAGE_SIZE)); // should always be true

        // Check if the new top of the heap would exceed the user stack.
        let new_top = proc.heap.mapped_end() + mapped_increment;
        if new_top > self.memory_layout().default_user_stack().start {
            return Err(BreakError::OutOfMemory);
        }

        // Map the new pages
        self.loader()
            .map_range_alloc(
                proc.aspace.page_table_walker(),
                proc.heap.mapped_end(),
                mapped_increment,
                EntryFlags::RW | EntryFlags::USER | EntryFlags::ACCESS,
            )
            .map_err(|_| BreakError::OutOfMemory)?;

        // Zero the new pages
        self.loader()
            .zero_user(&mut proc.aspace, proc.heap.mapped_end(), mapped_increment)
            .expect("failed to zero new pages");

        proc.heap.extend_reservation(increment, mapped_increment);

        Ok(prev_brk)
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

        let ksp = layout.kernel_stack.initial_sp.as_usize();
        let mut astate = ProcState {
            ti: ThreadInfo {
                ksp,
                usp: parent.astate.ti.usp,
            },
            tf: parent.astate.tf.clone(),
            // First switch into the child lands in `return_to_user`, which `sret`s to user.
            ctx: initial_context(ksp),
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
            heap: parent.heap,       // Inherit the heap from the parent
            fds: parent.fds.clone(), // Inherit the file descriptor table from the parent
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

/// Returns the ELF loader for RISC-V processes.
pub fn process_loader() -> impl ElfLoader {
    RiscvLoader
}

/// Returns the process builder for RISC-V.
pub fn process_builder() -> impl ProcessBuilder {
    RiscvProcessBuilder {
        loader: RiscvLoader,
        executor: RiscvUserProcessExecutor,
        memory_layout: RiscvProcessMemoryLayout,
    }
}
