//! Process management module.

use alloc::vec::Vec;
use spin::Mutex;

use crate::{
    arch::hal,
    mm::addr::VirtAddr,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment},
};

pub mod elf;
pub mod sched;

/// A user process.
pub struct Process {
    /// The current state of the process, including registers and other architecture-specific information.
    pub state: hal::proc::ProcState,

    /// The address space of the process, which defines its virtual memory layout.
    pub aspace: hal::proc::AddrSpace,
}

/// A unique identifier for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId {
    idx: usize,
    generation: usize,
}

impl ProcessId {
    /// Returns the index of the process in the process table.
    pub fn pid(self) -> usize {
        self.idx
    }
}

/// The process table, which stores all processes in the system and allows for allocation,
/// retrieval, and deallocation of processes.
pub struct ProcessTable {
    slots: Vec<ProcessSlot>,
    free: Vec<usize>,
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
    pub fn free(&mut self, pid: ProcessId) {
        if let Some(slot) = self.slots.get_mut(pid.idx)
            && slot.generation == pid.generation
        {
            // Invalidate the slot by incrementing the generation and removing the process
            slot.generation += 1;
            slot.process = None;
            // Add the index back to the free list for reuse
            self.free.push(pid.idx);
        }
    }
}

// Global process table, protected by a mutex for safe concurrent access.
static PROCESS_TABLE: Mutex<ProcessTable> = Mutex::new(ProcessTable::new());

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

    /// Creates a new process by duplicating the given parent process.
    fn fork(&self, parent: &Process) -> Process;

    /// Loads and executes a process given its ELF representation.
    ///
    /// The default implementation is fine for most cases. Each implementor can override it
    /// for finer grained control over process execution.
    fn exec(&self, bytes: &[u8]) -> ! {
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
            aspace,
            state: hal::proc::ProcState::default(),
        };

        // Start execution of the new process
        // SAFETY: we have just created and loaded the address space for this process
        unsafe { self.executor().enter_user(proc, plan.entry, stack_layout) };
    }
}

/// Trait for executing user processes on the current architecture.
pub trait UserProcessExecutor {
    /// The type representing the process's address space.
    /// This is typically the same as the `AddrSpace` associated type from `ElfLoader`.
    type AddrSpace;

    /// Enters user mode for the specified address space, starting execution of the
    /// process at the given entry point and stack layout.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address space is properly set up for user execution,
    /// and that the entry point and stack pointers are valid for the user process.
    unsafe fn enter_user(&self, proc: Process, entry: VirtAddr, stack: ProcessStackLayout) -> !;
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

/// Forks the currently running process, creating a new child process that is a duplicate of the parent.
/// Returns the `ProcessId` of the newly created child process.
pub fn fork_current_process() -> ProcessId {
    let parent_pid =
        sched::current_process_id().expect("sys_fork called without a current process");

    fork_process(parent_pid)
}

fn fork_process(parent_pid: ProcessId) -> ProcessId {
    let child_proc = {
        let proc_table = PROCESS_TABLE.lock();
        let parent_proc = proc_table.get(parent_pid).expect("invalid parent PID");
        hal::proc::builder().fork(parent_proc)
    };
    sched::allocate_process(child_proc)
}

/// Returns a reference to the global process table, protected by a mutex for safe concurrent access.
pub fn global_process_table() -> &'static Mutex<ProcessTable> {
    &PROCESS_TABLE
}
