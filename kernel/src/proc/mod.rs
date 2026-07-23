//! Process management module.

use core::{error::Error, fmt};

use alloc::vec::Vec;

use crate::{
    arch::hal,
    mm::addr::VirtAddr,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment},
    sync::IrqSpinLock,
    vfs::fd::FdTable,
};

pub mod elf;
pub mod sched;

/// A user process.
pub struct Process {
    /// The kind of process, which can be either a user process or a kernel thread.
    pub kind: ProcessKind,

    /// The current execution state.
    pub state: ProcessState,

    /// The current state of the process, including registers and other architecture-specific information.
    pub astate: hal::proc::ProcArchState,

    /// The address space of the process, which defines its virtual memory layout.
    pub aspace: hal::proc::AddrSpace,

    /// The unique process identifier (PID) for this process.
    pub pid: Pid,

    /// The parent process ID, if any. This is used to track the process hierarchy.
    pub parent: Option<ProcessId>,

    /// The list of child process IDs. This is used to manage the process tree and for cleanup when a process exits.
    pub children: Vec<ProcessId>,

    /// Heap information for the process.
    pub heap: ProcessHeap,

    /// Open file descriptors for the process.
    pub fds: FdTable,
}

/// Logical process identifier.
///
/// This is a unique, stable identifier for a process in the system. It is assigned at creation
/// and persists for the lifetime of the process. It is mainly used to reference a process
/// in system calls from user space.
///
/// [`ProcessId`] identifies a slot in the process table; [`Pid`] is the numerical id exposed to
/// userspace and used for init identification ([`Pid::INIT`] == 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid(usize);

impl Pid {
    /// Logical PID reserved for the userspace init process.
    pub const INIT: Pid = Pid(1);

    /// Returns the raw numerical value of this PID.
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<Pid> for usize {
    fn from(pid: Pid) -> Self {
        pid.as_usize()
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A discriminant between user and kernel processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessKind {
    /// The process is a user-space process, running in user mode with its own address space.
    User,
    /// The process is a kernel thread, running in kernel mode and sharing the kernel address space.
    Kernel,
}

/// The execution state of a process.
pub enum ProcessState {
    /// The process is currently running and can be scheduled by the scheduler.
    Running,

    /// The process is waiting for an event, such as I/O completion or a signal.
    Waiting,

    /// The process has exited and is waiting for its parent to collect its exit status.
    Zombie {
        /// The exit code of the process.
        exit_code: usize,
    },
}

/// The heap of a process, which manages the program break and the mapped end of the heap.
#[derive(Debug, Clone, Copy)]
pub struct ProcessHeap {
    /// The base virtual address of the heap. This is the starting point for heap allocations.
    base: VirtAddr,

    /// The current end of the heap (program break) for the process.
    brk: VirtAddr,

    /// The highest virtual address that has been mapped for the heap.
    mapped_end: VirtAddr,
}

impl ProcessHeap {
    /// Creates a new `ProcessHeap` with the specified base virtual address.
    /// The initial program break and mapped end of the heap are set to the base address,
    /// i.e. the heap is empty and has no additional capacity.
    pub const fn new(base: VirtAddr) -> Self {
        ProcessHeap {
            base,
            brk: base,
            mapped_end: base,
        }
    }

    /// Returns the current program break (end of the heap) for the process.
    pub fn brk(&self) -> VirtAddr {
        self.brk
    }

    /// Returns the highest virtual address that has been mapped for the heap.
    pub fn mapped_end(&self) -> VirtAddr {
        self.mapped_end
    }

    /// Returns the available space in the heap, which is the difference between the mapped end
    /// and the current program break. No new allocations are required to reserve heap memory
    /// if the requested increment is less than or equal to the available space.
    pub fn available_space(&self) -> usize {
        self.mapped_end.as_usize() - self.brk.as_usize()
    }

    /// Returns the total allocated size of the heap, which is the difference between the current
    /// program break and the base of the heap. This represents the total amount of heap memory
    /// that has been allocated for the process, regardless of whether it is currently in use.
    pub fn allocated_size(&self) -> usize {
        self.brk.as_usize() - self.base.as_usize()
    }

    /// Attempts to reserve the specified increment of heap space without mapping new pages.
    /// If there is enough available space, the program break is adjusted and the previous program break
    /// is returned. If there is not enough space, `None` is returned.
    pub fn try_reserve(&mut self, increment: usize) -> Option<VirtAddr> {
        if self.available_space() >= increment {
            let prev_brk = self.brk;
            self.brk = self.brk + increment;
            Some(prev_brk)
        } else {
            None
        }
    }

    /// Reclaims the specified increment of heap space, reducing the program break.
    /// It does not modify the mapped end of the heap, so future allocations may successfully use
    /// the reclaimed space if it is still within the mapped range.
    ///
    /// Returns the previous program break on success, or an error if the reclaim increment is
    /// larger than the total allocated size of the heap.
    pub fn reclaim(&mut self, increment: usize) -> Result<VirtAddr, BreakError> {
        if increment > self.allocated_size() {
            return Err(BreakError::InvalidIncrement);
        }
        let prev_brk = self.brk;
        self.brk = self.brk - increment;
        assert!(
            self.brk >= self.base,
            "program break cannot be less than the base of the heap"
        );
        Ok(prev_brk)
    }

    /// Extends the heap reservation by the specified increment and mapped increment.
    /// This function must be called after new pages have been mapped to extend the heap.
    pub fn extend_reservation(&mut self, increment: usize, mapped_increment: usize) {
        self.brk = self.brk + increment;
        self.mapped_end = self.mapped_end + mapped_increment;
    }
}

/// A unique identifier for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId {
    idx: usize,
    generation: usize,
}

/// The process table, which stores all processes in the system and allows for allocation,
/// retrieval, and deallocation of processes.
pub struct ProcessTable {
    slots: Vec<ProcessSlot>,
    free: Vec<usize>,
    next_pid: usize,
    /// Cached table handle for the userspace init process ([`Pid::INIT`]).
    init_id: Option<ProcessId>,
}

/// A versioned slot in the process table, which can either be occupied by a process or free.
#[derive(Default)]
struct ProcessSlot {
    generation: usize,
    process: Option<Process>,
}

impl Default for ProcessTable {
    fn default() -> Self {
        ProcessTable::new()
    }
}

impl ProcessTable {
    /// Creates a new, empty process table.
    pub const fn new() -> Self {
        ProcessTable {
            slots: Vec::new(),
            free: Vec::new(),
            next_pid: 2, // 0 unused, 1 reserved for init (`Pid::INIT`)
            init_id: None,
        }
    }

    /// Allocates a new process in the table and returns its unique identifier.
    /// If there are free slots available, it reuses one; otherwise, it creates a new slot.
    pub fn allocate(&mut self, p: Process) -> ProcessId {
        // Try to reuse a free slot, or create a new one if none are available.
        let idx = self.free.pop().unwrap_or_else(|| {
            self.slots.push(ProcessSlot::default());
            self.slots.len() - 1
        });
        let generation = self.slots[idx].generation;
        // Store the new process in the slot
        self.slots[idx].process = Some(p);
        ProcessId { idx, generation }
    }

    /// Retrieves a reference to the process associated with the given `ProcessId`,
    /// if it exists and is valid.
    pub fn get(&self, pid: ProcessId) -> Option<&Process> {
        self.slots.get(pid.idx).and_then(|slot| {
            if slot.generation == pid.generation {
                slot.process.as_ref()
            } else {
                None
            }
        })
    }

    /// Retrieves a mutable reference to the process associated with the given `ProcessId`,
    /// if it exists and is valid.
    pub fn get_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.slots.get_mut(pid.idx).and_then(|slot| {
            if slot.generation == pid.generation {
                slot.process.as_mut()
            } else {
                None
            }
        })
    }

    /// Frees the process associated with the given `ProcessId`, if it exists and is valid.
    pub fn take(&mut self, pid: ProcessId) -> Option<Process> {
        if let Some(slot) = self.slots.get_mut(pid.idx)
            && slot.generation == pid.generation
        {
            // Invalidate the slot by incrementing the generation and removing the process
            slot.generation += 1;
            let process = slot.process.take();
            // Add the index back to the free list for reuse
            self.free.push(pid.idx);
            process
        } else {
            None
        }
    }

    /// Removes all exited kernel threads from the table.
    ///
    /// Callers must only destroy these after the thread has switched away from its stack
    /// (typically from idle).
    pub fn take_zombie_kthreads(&mut self) -> Vec<Process> {
        let mut ids = Vec::new();
        for (idx, slot) in self.slots.iter().enumerate() {
            if let Some(p) = slot.process.as_ref()
                && matches!(p.kind, ProcessKind::Kernel)
                && matches!(p.state, ProcessState::Zombie { .. })
            {
                ids.push(ProcessId {
                    idx,
                    generation: slot.generation,
                });
            }
        }
        ids.into_iter().filter_map(|id| self.take(id)).collect()
    }

    /// Allocates a new unique logical process identifier.
    ///
    /// Never returns [`Pid::INIT`] (reserved for userspace init via [`ProcessBuilder::spawn_init`]).
    pub fn alloc_pid(&mut self) -> Pid {
        debug_assert_ne!(self.next_pid, Pid::INIT.as_usize());
        let pid = Pid(self.next_pid);
        self.next_pid += 1;
        pid
    }

    /// Caches the process-table handle for userspace init.
    ///
    /// # Panics
    ///
    /// Debug builds panic if init was already registered, `id` is missing from the table, or the
    /// process does not have logical [`Pid::INIT`].
    pub fn set_init_id(&mut self, id: ProcessId) {
        debug_assert!(self.init_id.is_none(), "init process already registered");
        let proc = self.get(id).expect("init process not in table");
        debug_assert_eq!(proc.pid, Pid::INIT, "init must have logical Pid::INIT");
        self.init_id = Some(id);
    }

    /// Returns the cached [`ProcessId`] of the userspace init process, if it has been set.
    pub fn init_id(&self) -> Option<ProcessId> {
        self.init_id
    }
}

// Global process table, protected by a spinlock for safe concurrent access.
static PROCESS_TABLE: IrqSpinLock<ProcessTable> = IrqSpinLock::new(ProcessTable::new());

/// A trait to implement a user space process builder and executor.
pub trait ProcessBuilder {
    /// The process loader
    type Loader: ElfLoader<AddrSpace = hal::proc::AddrSpace>;

    /// The process executor
    type Executor: UserProcessExecutor<AddrSpace = hal::proc::AddrSpace>;

    /// The process memory layout
    type MemoryLayout: ProcessMemoryLayout;

    /// Returns a reference to the loader.
    fn loader(&self) -> &Self::Loader;

    /// Returns a reference to the executor.
    fn executor(&self) -> &Self::Executor;

    /// Returns a reference to the memory layout.
    fn memory_layout(&self) -> &Self::MemoryLayout;

    /// Sets up the initial stack memory for the process.
    ///
    /// This is called during process execution after the ELF binary has been loaded, and allows the
    /// builder to set up the user and kernel stacks according to the architecture's requirements.ù
    /// The returned layout must match the allocated stack memory regions.
    fn setup_stack_memory(
        &self,
        aspace: &mut hal::proc::AddrSpace,
    ) -> Result<ProcessStackLayout, ElfLoadError>;

    /// Adjusts the program break (heap size) for the given process by the specified increment.
    ///
    /// Returns the previous program break address on success, or an error if the operation fails.
    fn adjust_program_break(
        &self,
        proc: &mut Process,
        increment: isize,
    ) -> Result<VirtAddr, BreakError>;

    /// Creates a new process by duplicating the given parent process.
    fn fork(&self, parent: &Process) -> Process;

    /// Destroys the given process, cleaning up any resources associated with it.
    fn destroy(&self, process: Process);

    /// Loads a user process from its ELF image and enqueues it for scheduling.
    ///
    /// Returns the new process id. Does not enter the idle loop; the caller must
    /// invoke the architecture idle/scheduler entry separately (e.g. after boot).
    ///
    /// The default implementation is fine for most cases. Each implementor can override it
    /// for finer grained control over process creation.
    fn spawn_user(&self, bytes: impl AsRef<[u8]>) -> ProcessId {
        let logical_pid = PROCESS_TABLE.lock().alloc_pid();
        self.spawn_user_with_pid(bytes, logical_pid)
    }

    /// Spawns the userspace init process with logical [`Pid::INIT`] and registers it for
    /// orphan reparenting.
    ///
    /// Must be called at most once. Prefer this over [`Self::spawn_user`] for `/init`.
    fn spawn_init(&self, bytes: impl AsRef<[u8]>) -> ProcessId {
        let id = self.spawn_user_with_pid(bytes, Pid::INIT);
        PROCESS_TABLE.lock().set_init_id(id);
        id
    }

    /// Shared spawn path used by [`Self::spawn_user`] and [`Self::spawn_init`].
    fn spawn_user_with_pid(&self, bytes: impl AsRef<[u8]>, logical_pid: Pid) -> ProcessId {
        let bytes = bytes.as_ref();

        // Create a new user address space
        let mut aspace = match self.loader().new_user_addr_space() {
            Ok(aspace) => aspace,
            Err(e) => {
                panic!("failed to create user address space: {:?}", e);
            }
        };

        let mut seg_buf = [LoadSegment::default(); 16];

        // Load ELF into the new address space
        let plan = match elf::load_elf_into(
            self.loader(),
            &mut aspace,
            bytes,
            elf::LoadPolicy {
                allow_wx: false,
                pie_base_hint: 0,
                max_segments: seg_buf.len(),
            },
            &mut seg_buf,
        ) {
            Ok(plan) => plan,
            Err(e) => {
                panic!("failed to load ELF for process: {:?}", e);
            }
        };

        // Setup initial stack
        let stack_layout = match self.setup_stack_memory(&mut aspace) {
            Ok(layout) => layout,
            Err(e) => {
                panic!("failed to set up stack memory: {:?}", e);
            }
        };

        // Create the process and add it to the scheduler
        let proc = Process {
            kind: ProcessKind::User,
            state: ProcessState::Running,
            aspace,
            astate: hal::proc::ProcArchState::default(),
            pid: logical_pid,
            parent: None,
            children: Vec::new(),
            heap: ProcessHeap::new(plan.heap_start),
            fds: FdTable::with_stdio(),
        };

        // SAFETY: we have just created and loaded the address space for this process
        unsafe { self.executor().enqueue_user(proc, plan.entry, stack_layout) }
    }
}

/// Trait for preparing and enqueuing user processes on the current architecture.
pub trait UserProcessExecutor {
    /// The type representing the process's address space.
    /// This is typically the same as the `AddrSpace` associated type from `ElfLoader`.
    type AddrSpace;

    /// Prepares arch state for `proc` and enqueues it; returns its process id.
    ///
    /// Does not enter user mode or the idle loop. The first schedule of this process
    /// lands in the return-to-user trampoline.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address space is properly set up for user execution,
    /// and that the entry point and stack pointers are valid for the user process.
    unsafe fn enqueue_user(
        &self,
        proc: Process,
        entry: VirtAddr,
        stack: ProcessStackLayout,
    ) -> ProcessId;
}

/// Specification of a user stack layout.
pub struct StackSpec {
    /// The start virtual address of the stack (lowest address).
    pub start: VirtAddr,
    /// The end virtual address of the stack (highest address).
    pub end: VirtAddr,
    /// The initial stack pointer value.
    pub initial_sp: VirtAddr,
}

/// Specification of the kernel and user stack layout for a process.
pub struct ProcessStackLayout {
    /// The user stack specification.
    pub user_stack: StackSpec,
    /// The kernel stack specification.
    pub kernel_stack: StackSpec,
}

/// Trait defining the default memory layout for user processes on the current architecture.
pub trait ProcessMemoryLayout {
    /// Returns the default stack layout for user processes, including both user and kernel stacks.
    fn default_stack_layout(&self) -> ProcessStackLayout;
}

/// Possible errors when loading a process.
#[derive(Debug, Clone, Copy)]
pub enum ProcessLoadError {
    /// Architecture-specific loading error.
    ArchError,
    /// ELF loading error.
    ElfLoadError(ElfLoadError),
}

impl From<ElfLoadError> for ProcessLoadError {
    fn from(e: ElfLoadError) -> Self {
        ProcessLoadError::ElfLoadError(e)
    }
}

/// Error type for process heap adjustment (program break) operations.
#[derive(Debug, Clone, Copy)]
pub enum BreakError {
    /// The increment is invalid.
    InvalidIncrement,
    /// Cannot allocate additional memory.
    OutOfMemory,
}

impl fmt::Display for BreakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BreakError::InvalidIncrement => write!(f, "invalid increment"),
            BreakError::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

impl Error for BreakError {}

/// Executes a closure with access to the currently running process.
pub fn with_current_process<F, R>(f: F) -> R
where
    F: FnOnce(&Process) -> R,
{
    let pid = sched::current_process_id().expect("no current process");
    let proc_table = PROCESS_TABLE.lock();
    let proc = proc_table.get(pid).expect("invalid PID");
    f(proc)
}

/// Executes a closure with mutable access to the currently running process.
pub fn with_current_process_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut Process) -> R,
{
    let pid = sched::current_process_id().expect("no current process");
    let mut proc_table = PROCESS_TABLE.lock();
    let proc = proc_table.get_mut(pid).expect("invalid PID");
    f(proc)
}

/// Forks the currently running process, creating a new child process that is a duplicate of the parent.
///
/// Returns the child's process-table handle and its logical [`Pid`].
pub fn fork_current_process() -> (ProcessId, Pid) {
    let parent_pid =
        sched::current_process_id().expect("sys_fork called without a current process");

    fork_process(parent_pid)
}

fn fork_process(parent_pid: ProcessId) -> (ProcessId, Pid) {
    let child_proc = {
        let mut proc_table = PROCESS_TABLE.lock();
        let parent_proc = proc_table.get_mut(parent_pid).expect("invalid parent PID");
        let mut child_proc = hal::proc::builder().fork(parent_proc);
        child_proc.parent = Some(parent_pid);
        child_proc
    };
    let logical_pid = child_proc.pid;
    let child_id = sched::allocate_process(child_proc);
    {
        let mut proc_table = PROCESS_TABLE.lock();
        let parent_proc = proc_table.get_mut(parent_pid).expect("invalid parent PID");
        parent_proc.children.push(child_id);
    }
    (child_id, logical_pid)
}

/// Exits the current kernel thread and switches to idle.
///
/// Marks the thread as a zombie without reparenting children or waking a parent.
/// Idle reaps the zombie and frees its heap-backed stack after this switch returns there.
///
/// # Panics
///
/// Panics if there is no current process or if the current process is not a kernel thread.
pub fn kthread_exit() -> ! {
    let pid = sched::current_process_id().expect("kthread_exit: no current process");

    {
        let mut proc_table = PROCESS_TABLE.lock();
        let proc = proc_table.get_mut(pid).expect("kthread_exit: invalid PID");
        assert!(
            matches!(proc.kind, ProcessKind::Kernel),
            "kthread_exit: current process is not a kernel thread"
        );
        proc.state = ProcessState::Zombie { exit_code: 0 };
    }

    sched::exit_current(pid);
    // Return to idle (not `switch_from_exiting`) so idle can reap the heap stack.
    hal::proc::switch(Some(pid), None);
    panic!("kthread_exit: resumed after switch to idle");
}

/// Reaps exited kernel threads and frees their heap-backed stacks.
///
/// Must run on a stack other than the zombies' own (idle calls this each loop).
pub fn reap_zombie_kthreads() {
    let zombies = {
        let mut table = PROCESS_TABLE.lock();
        table.take_zombie_kthreads()
    };
    for proc in zombies {
        hal::proc::builder().destroy(proc);
    }
}

/// Exits the currently running process and transfers control to the scheduler.
/// This function does not return, as the current process is terminated.
pub fn exit_current(exit_code: usize) -> ! {
    let pid = sched::current_process_id().expect("sys_exit called without a current process");

    // Remove the current process from the scheduler
    sched::exit_current(pid);

    // Mark the process as a zombie and store its exit code.
    // Also reparent any child processes to userspace init to ensure they are not orphaned.
    let mut wake_parent = None;
    {
        let mut proc_table = PROCESS_TABLE.lock();
        mark_as_zombie(&mut proc_table, pid, exit_code);
        reparent_children(&mut proc_table, pid);

        // Wake up parent process that might be waiting for this child to exit
        let proc = proc_table.get(pid).expect("invalid PID");
        if let Some(parent_pid) = proc.parent
            && let Some(parent_proc) = proc_table.get_mut(parent_pid)
            && matches!(parent_proc.state, ProcessState::Waiting)
        {
            parent_proc.state = ProcessState::Running;
            wake_parent = Some(parent_pid); // Mark parent to be woken up after releasing the lock
        }
    }

    if let Some(parent_pid) = wake_parent {
        sched::enqueue_process(parent_pid); // parent already marked as Running
    }

    // Switch to the next runnable process (may resume a parked waiter via swtch).
    sched::switch_from_exiting(pid);
}

fn mark_as_zombie(proc_table: &mut ProcessTable, pid: ProcessId, exit_code: usize) {
    let proc = proc_table.get_mut(pid).expect("invalid PID");
    proc.state = ProcessState::Zombie { exit_code };
}

fn reparent_children(proc_table: &mut ProcessTable, pid: ProcessId) {
    let parent_proc = proc_table.get_mut(pid).expect("invalid PID");
    let children = core::mem::take(&mut parent_proc.children);

    let init_id = proc_table.init_id().expect("init process not found");

    for child_pid in children {
        if let Some(child_proc) = proc_table.get_mut(child_pid) {
            child_proc.parent = Some(init_id);
            let init_proc = proc_table.get_mut(init_id).expect("init process not found");
            init_proc.children.push(child_pid);
        }
    }
}

/// Returns a reference to the global process table, protected by a spinlock for safe concurrent access.
pub fn global_process_table() -> &'static IrqSpinLock<ProcessTable> {
    &PROCESS_TABLE
}

/// Returns the process-table handle for userspace init.
///
/// # Panics
///
/// Panics if init has not been spawned yet.
pub fn init_process_id() -> ProcessId {
    PROCESS_TABLE
        .lock()
        .init_id()
        .expect("init process not set")
}

/// Spawns a kernel thread that runs `entry(arg)` in S-mode on a private kernel stack.
///
/// Uses the shared global kernel page tables and a heap-backed stack. Does not switch to the
/// new thread; the caller (or idle) must schedule it. If `entry` returns, the trampoline
/// calls [`kthread_exit`].
pub fn spawn_kthread(entry: fn(usize), arg: usize) -> ProcessId {
    hal::proc::spawn_kthread(entry, arg)
}
