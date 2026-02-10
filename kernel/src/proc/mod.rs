//! Process management module.

use crate::{
    mm::addr::VirtAddr,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment},
};

pub mod elf;

/// A trait to implement a user space process builder and executor.
pub trait ProcessBuilder {
    /// The process' address space
    type AddrSpace;

    /// The process loader
    type Loader: ElfLoader<AddrSpace = Self::AddrSpace>;

    /// The process executor
    type Executor: UserProcessExecutor<AddrSpace = Self::AddrSpace>;

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
        aspace: &mut Self::AddrSpace,
    ) -> Result<ProcessStackLayout, ElfLoadError>;

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

        // Start execution of the new process
        // SAFETY: we have just created and loaded the address space for this process
        unsafe {
            self.executor()
                .enter_user(&aspace, plan.entry, stack_layout)
        };
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
    unsafe fn enter_user(
        &self,
        aspace: &Self::AddrSpace,
        entry: VirtAddr,
        stack: ProcessStackLayout,
    ) -> !;

    /// Resumes execution of a user process in the specified address space.
    ///
    /// # Safety
    ///
    /// See [`enter_user`].
    unsafe fn resume_user(&self, aspace: &Self::AddrSpace) -> !;
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
