use crate::{
    arch::{
        PAGE_SIZE,
        riscv::{
            addr::VirtAddr,
            mm::{GFA, MAPPER},
            mmu::{EntryFlags, PageSize, PageTable, PageTableWalker},
        },
    },
    mm::allocator::FrameAllocator,
    proc::{Process, ProcessMemory},
};

/// Allocates and maps memory for a new userspace process.
/// Returns the frames allocated for the process.
pub fn alloc_process_memory(p: &mut Process) -> ProcessMemory {
    // TODO: remove hard-coded addresses
    const PROC_TEXT_VA: VirtAddr = VirtAddr::new_truncated(0x4000_0000);
    const PROC_DATA_VA: VirtAddr = VirtAddr::new_truncated(0x4000_1000);
    const PROC_STACK_VA: VirtAddr = VirtAddr::new_truncated(0x4000_2000);

    let code_frame;
    let data_frame;
    let stack_frame;

    // SAFETY: `GFA` has been initialized in `setup_late`.
    unsafe {
        code_frame = GFA.lock().as_mut().unwrap().alloc(1).expect("oom");
        data_frame = GFA.lock().as_mut().unwrap().alloc(1).expect("oom");
        stack_frame = GFA.lock().as_mut().unwrap().alloc(1).expect("oom");
    };

    let mut gfa = GFA.lock();
    let gfa = gfa.as_mut().unwrap();

    // Let's start by getting a new root page table and its walker.
    // SAFETY: if we have correctly set up the frame allocator, this is safe
    let (mut user_mapper, user_rpt_pa) = unsafe {
        let rpt_frame = gfa.alloc(1).unwrap();

        let rpt = rpt_frame.ptr as *mut PageTable;
        rpt.write(PageTable::new());

        (PageTableWalker::new(&mut *rpt), rpt_frame.paddr)
    };

    // SAFETY: `MAPPER.page_table()` is the kernel root page directory.
    unsafe {
        let kernel_rpt = MAPPER.lock();
        let kernel_rpt = kernel_rpt.as_ref().unwrap().page_table();

        // Copy kernel mappings
        user_mapper.copy_kernel_mappings(kernel_rpt, gfa).unwrap();

        // Map user code pages
        user_mapper
            .map(
                PROC_TEXT_VA,
                code_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RX,
                gfa,
            )
            .unwrap();

        // Map user data pages
        user_mapper
            .map(
                PROC_DATA_VA,
                data_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RW,
                gfa,
            )
            .unwrap();

        // Map user stack pages
        user_mapper
            .map(
                PROC_STACK_VA,
                stack_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RW,
                gfa,
            )
            .unwrap();
    }

    // Store the root page table physical address
    p.rpt_pa = user_rpt_pa.data();

    ProcessMemory {
        text_frame: code_frame.paddr.into(),
        data_frame: data_frame.paddr.into(),
        stack_frame: stack_frame.paddr.into(),
        text_start_va: PROC_TEXT_VA.data(),
        stack_top_va: PROC_STACK_VA.data() + PAGE_SIZE as usize,
    }
}
