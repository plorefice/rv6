//! Process management module.

use crate::{
    mm::addr::VirtAddr,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment, SegmentFlags},
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

    /// Loads and executes a process given its ELF representation.
    ///
    /// The default implementation is fine for most cases. Each implementor can override it
    /// for finer grained control over process execution.
    fn exec(&self, bytes: &[u8]) -> Result<(), ProcessLoadError> {
        // Create a new user address space
        let mut aspace = self
            .loader()
            .new_user_addr_space()
            .map_err(|_| ProcessLoadError::ArchError)?;

        let mut seg_buf = [LoadSegment::default(); 16];

        // Load ELF into the new address space
        let plan = elf::load_elf_into(
            self.loader(),
            &mut aspace,
            bytes,
            elf::LoadPolicy {
                allow_wx: false,
                pie_base_hint: 0,
                max_segments: seg_buf.len(),
            },
            &mut seg_buf,
        )?;

        // Set up user stack
        let stack = self.memory_layout().default_stack();
        self.loader()
            .map_anonymous(
                &mut aspace,
                stack.start,
                (stack.end - stack.start).as_usize(),
                SegmentFlags::R | SegmentFlags::W,
            )
            .map_err(|_| ProcessLoadError::ArchError)?;

        // Start execution of the new process
        // SAFETY: we have just created and loaded the address space for this process
        unsafe {
            self.executor()
                .enter_user(&aspace, plan.entry, stack.initial_sp)
        };
    }
}

/// Trait for executing user processes on the current architecture.
pub trait UserProcessExecutor {
    /// The type representing the process's address space.
    /// This is typically the same as the `AddrSpace` associated type from `ElfLoader`.
    type AddrSpace;

    /// Enters user mode for the specified address space, starting execution of the
    /// process at the given entry point and stack pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address space is properly set up for user execution,
    /// and that the entry point and stack pointer are valid for the user process.
    unsafe fn enter_user(&self, aspace: &Self::AddrSpace, entry: VirtAddr, sp: VirtAddr) -> !;

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

/// Trait defining the default memory layout for user processes on the current architecture.
pub trait ProcessMemoryLayout {
    /// Returns the highest valid user virtual address.
    fn user_end(&self) -> VirtAddr;

    /// Returns the default stack specification for user processes.
    fn default_stack(&self) -> StackSpec;
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
