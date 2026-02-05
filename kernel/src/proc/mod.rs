//! Process management module.

use crate::{
    arch::ArchServices,
    mm::addr::VirtAddr,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment, SegmentFlags},
};

pub mod elf;

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

/// Loads an executable file as a new process and starts its execution.
pub fn execve(arch: &ArchServices, bytes: &[u8]) -> Result<(), ProcessLoadError> {
    // Create a new user address space
    let aspace = &mut arch
        .loader
        .new_user_addr_space()
        .map_err(|_| ProcessLoadError::ArchError)?;

    let mut seg_buf = [LoadSegment::default(); 16];

    // Load ELF into the new address space
    let plan = elf::load_elf_into(
        &arch.loader,
        aspace,
        bytes,
        elf::LoadPolicy {
            allow_wx: false,
            pie_base_hint: 0,
            max_segments: seg_buf.len(),
        },
        &mut seg_buf,
    )?;

    // Set up user stack
    let stack = arch.uml.default_stack();
    arch.loader
        .map_anonymous(
            aspace,
            stack.start,
            (stack.end - stack.start).as_usize(),
            SegmentFlags::R | SegmentFlags::W,
        )
        .map_err(|_| ProcessLoadError::ArchError)?;

    // Start execution of the new process
    // SAFETY: we have just created and loaded the address space for this process
    unsafe { arch.uexec.enter_user(aspace, plan.entry, stack.initial_sp) };
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
