//! Process management module.

use crate::arch::{self, pa_to_va};

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

/// Spawns a sample init process.
pub fn spawn_init_process() -> ! {
    let mut pcb = Process::new();

    // Allocate user memory
    let procmem = crate::arch::alloc_process_memory(&mut pcb);

    // Copy user code into place
    // TODO: actually load data from somewhere
    // SAFETY: `code_frame` was just allocated and mapped.
    unsafe {
        // addi a0, zero, 1
        // ecall
        let code: [u8; 8] = [0x13, 0x05, 0x10, 0x00, 0x73, 0x00, 0x00, 0x00];

        core::ptr::copy_nonoverlapping(
            code.as_ptr(),
            pa_to_va(procmem.text_frame).as_mut_ptr(),
            code.len(),
        );
    }

    // Start process execution
    // SAFETY: memory has been initialized right above.
    unsafe {
        arch::switch_to_process(&pcb, &procmem);
    }
}
