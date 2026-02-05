//! Process management module.

use crate::{
    mm::addr::{MemoryAddress, VirtAddr},
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

/// Loads an executable file as a new process and starts its execution.
pub fn execve<L, E, A>(loader: &L, executor: &E, bytes: &[u8]) -> Result<(), ProcessLoadError>
where
    L: ElfLoader<AddrSpace = A>,
    E: UserProcessExecutor<AddrSpace = A>,
{
    // Virtual address of the last valid user address.
    const USER_END: usize = 0x0000_003f_ffff_f000;

    // Set up user stack at the top of the user address space
    const STACK_TOP: usize = USER_END;
    const STACK_SIZE: usize = 8 * 1024 * 1024; // 8 MiB
    const STACK_BOTTOM: usize = STACK_TOP - STACK_SIZE;

    // Create a new user address space
    let aspace = &mut loader
        .new_user_addr_space()
        .map_err(|_| ProcessLoadError::ArchError)?;

    let mut seg_buf = [LoadSegment::default(); 16];

    // Load ELF into the new address space
    let plan = elf::load_elf_into(
        loader,
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
    loader
        .map_anonymous(
            aspace,
            VirtAddr::new(STACK_BOTTOM),
            STACK_SIZE,
            SegmentFlags::R | SegmentFlags::W,
        )
        .map_err(|_| ProcessLoadError::ArchError)?;

    // Start execution of the new process
    // SAFETY: we have just created and loaded the address space for this process
    unsafe { executor.enter_user(aspace, plan.entry, VirtAddr::new(STACK_TOP)) };
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
