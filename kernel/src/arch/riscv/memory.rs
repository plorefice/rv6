use kmm::{
    allocator::{bitmap::BitmapAllocator, AllocatorError, FrameAllocator, LockedAllocator},
    Align,
};
use riscv::{
    addr::PAGE_SIZE,
    mmu::{self, EntryFlags, PageSize, PageTable},
    registers::{Satp, SatpMode},
    PhysAddr, VirtAddr,
};

// Defined in linker script
extern "C" {
    /// The starting word of the kernel in memory.
    static __start: usize;
    /// The ending word of the kernel in memory.
    static __end: usize;
    /// The starting word of the text section in memory.
    static __text_start: usize;
    /// The ending word of the text section in memory.
    static __text_end: usize;
    /// The starting word of the RO data section in memory.
    static __rodata_start: usize;
    /// The ending word of the RO data section in memory.
    static __rodata_end: usize;
    /// The starting word of the data section in memory.
    static __data_start: usize;
    /// The ending word of the data section in memory.
    static __data_end: usize;
}

/// Global frame allocator (GFA).
pub static mut GFA: LockedAllocator<BitmapAllocator<PhysAddr, PAGE_SIZE>> = LockedAllocator::new();

/// Initializes a physical memory allocator on the specified memory range.
///
/// # Safety
///
/// The caller must guarantee that the memory being initialized isn't already in use by the system.
unsafe fn phys_init(mem_start: PhysAddr, mem_size: u64) -> Result<(), AllocatorError> {
    let mem_start = mem_start.align_up(PAGE_SIZE);
    let mem_end = (mem_start + mem_size).align_down(PAGE_SIZE);

    GFA.set_allocator(BitmapAllocator::<PhysAddr, PAGE_SIZE>::init(
        mem_start, mem_end,
    )?);

    Ok(())
}

/// Configures the virtual address space as expected by the kernel.
///
/// # Safety
///
/// There can be no guarantee that the memory being initialized isn't already in use by the system.
unsafe fn setup_vm(root: &mut PageTable) {
    let text_start = PhysAddr::new(&__text_start as *const usize as u64);
    let text_end = PhysAddr::new(&__text_end as *const usize as u64);
    let rodata_start = PhysAddr::new(&__rodata_start as *const usize as u64);
    let rodata_end = PhysAddr::new(&__rodata_end as *const usize as u64);
    let data_start = PhysAddr::new(&__data_start as *const usize as u64);
    let data_end = PhysAddr::new(&__data_end as *const usize as u64);

    kprintln!("Kernel memory map:");
    kprintln!("  [{} - {}] .text", text_start, text_end);
    kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
    kprintln!("  [{} - {}] .data", data_start, data_end);

    // Identity map all kernel sections
    mmu::identity_map_range(root, text_start, text_end, EntryFlags::RX, &mut GFA)
        .expect("failed to map kernel .text section");
    mmu::identity_map_range(root, rodata_start, rodata_end, EntryFlags::RX, &mut GFA)
        .expect("failed to map kernel .rodata section");
    mmu::identity_map_range(root, data_start, data_end, EntryFlags::RW, &mut GFA)
        .expect("failed to map kernel .data section");

    // Identity map UART0 memory
    mmu::identity_map_range(
        root,
        PhysAddr::new(0x1000_0000),
        PhysAddr::new(0x1000_0100),
        EntryFlags::RW,
        &mut GFA,
    )
    .expect("failed to map UART MMIO");

    // Identity map CLINT memory
    mmu::identity_map_range(
        root,
        PhysAddr::new(0x0200_0000),
        PhysAddr::new(0x0201_0000),
        EntryFlags::RW,
        &mut GFA,
    )
    .expect("failed to map CLINT MMIO");

    // Identity map SYSCON memory
    mmu::identity_map_range(
        root,
        PhysAddr::new(0x0010_0000),
        PhysAddr::new(0x0010_1000),
        EntryFlags::RW,
        &mut GFA,
    )
    .expect("failed to map SYSCON MMIO");

    // Map the whole physical address space to VA 0xffff_8880_0000_0000
    mmu::map(
        root,
        VirtAddr::new(0xffff_8880_0000_0000),
        PhysAddr::new(0),
        PageSize::Tb,
        EntryFlags::RWX,
        &mut GFA,
    )
    .expect("failed to map physical memory");

    // Enable MMU
    Satp::write_ppn(PhysAddr::new_unchecked(root as *const _ as u64).page_index());
    Satp::write_mode(SatpMode::Sv48);
}

/// Initializes the system memory, by setting up a frame allocator and enabling virtual memory.
///
/// # Safety
///
/// Memory safety basically does not exist before this point :)
pub unsafe fn init() {
    let kernel_mem_end = PhysAddr::new(&__end as *const usize as u64);
    // TODO: parse DTB to get the memory size
    let phys_mem_end = PhysAddr::new(0x8000_0000) + 128 * 1024 * 1024;

    phys_init(kernel_mem_end, (phys_mem_end - kernel_mem_end).into()).unwrap();

    // Setup root page table for virtual address translation
    let root = (u64::from(GFA.alloc_zeroed(1).unwrap()) as *mut PageTable)
        .as_mut()
        .unwrap();

    // From this point on, we are using virtual addressing
    setup_vm(root);
}
