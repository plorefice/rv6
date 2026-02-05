//! Process management module.

use crate::{
    arch::ArchLoaderImpl,
    proc::elf::{ElfLoadError, ElfLoader, LoadSegment},
};

pub mod elf;

/// Process control block.
pub struct Process {
    /// Root page table physical address
    pub rpt_pa: u64,
}

/// Memory frames and mappings recorded at creation time.
pub struct ProcessMemory {
    /// Physical address of the start of the .text section
    pub text_frame: u64,
    /// Physical address of the start of the .data section
    pub data_frame: u64,
    /// Physical address of the start of the .stack section
    pub stack_frame: u64,
    /// Virtual address at which .text is mapped
    pub text_start_va: usize,
    /// Virtual address of the top of the stack
    pub stack_top_va: usize,
}

impl Default for Process {
    fn default() -> Self {
        Self::new()
    }
}

impl Process {
    /// Creates a new empty process control block.
    pub fn new() -> Self {
        Process { rpt_pa: 0 }
    }
}

/// Loads an executable file as a new process and starts its execution.
pub fn execve(bytes: &[u8]) -> Result<(), ProcessLoadError> {
    // Create architecture-specific loader and address space
    let loader = &mut ArchLoaderImpl::default();
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

    // Start execution of the new process
    elf::exec(loader, aspace, &plan).map_err(|_| ProcessLoadError::ArchError)?;

    // exec should not return
    panic!("execve returned unexpectedly");
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
