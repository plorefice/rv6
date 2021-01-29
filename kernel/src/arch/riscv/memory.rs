use kmm::{
    allocator::{bitmap::BitmapAllocator, AllocatorError, FrameAllocator, LockedAllocator},
    Align,
};
use mmu::OffsetPageMapper;
use riscv::{
    addr::PAGE_SIZE,
    mmu::{self, EntryFlags, MapError, PageSize, PageTable},
    registers::{Satp, SatpMode},
    PhysAddr, VirtAddr,
};
use rvalloc::BumpAllocator;

/// Virtual memory offset at which the physical address space is mapped.
pub const PHYS_MEM_OFFSET: VirtAddr = VirtAddr::new_truncated(0x8000_0000_0000);

/// Virtual memory address of the beginning of the kernel heap.
const HEAP_MEM_START: VirtAddr = VirtAddr::new_truncated(0xCAFE_0000_0000);

/// Size of the heap in bytes (1 MiB)
const HEAP_MEM_SIZE: usize = 1024 * 1024;

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

/// Global heap allocator.
#[global_allocator]
static HEAP: BumpAllocator =
    BumpAllocator::new(HEAP_MEM_START.data(), HEAP_MEM_START.data() + HEAP_MEM_SIZE);

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

    // Setup virtual memory, and switch to virtual addressing from now on.
    let mut mapper = setup_vm().expect("failed to set up virtual memory");

    // Allocate memory for the heap
    setup_heap(
        &mut mapper,
        HEAP_MEM_START,
        HEAP_MEM_START + HEAP_MEM_SIZE,
        phys_mem_end - HEAP_MEM_SIZE as u64,
    )
    .expect("failed to setup heap");
}

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
/// The caller must guarantee that the physical memory being mapped isn't already in use by the
/// system.
unsafe fn setup_vm() -> Result<OffsetPageMapper<'static>, MapError> {
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

    // Allocate a frame for the root page table to be used for virtual address translation.
    // Since translation is not enabled here yet, we can use the frame's physical address directly.
    let rpt = (GFA.alloc(1).unwrap().data() as *mut PageTable)
        .as_mut::<'static>()
        .unwrap();

    rpt.clear();

    // Again, since translation is off, we use an offset mapper with zero offset.
    let mut mapper = OffsetPageMapper::new(rpt, VirtAddr::new(0));

    // Identity map all kernel sections
    mapper.identity_map_range(text_start, text_end, EntryFlags::RX, &mut GFA)?;
    mapper.identity_map_range(rodata_start, rodata_end, EntryFlags::RX, &mut GFA)?;
    mapper.identity_map_range(data_start, data_end, EntryFlags::RW, &mut GFA)?;

    // Identity map UART0 memory
    mapper.identity_map_range(
        PhysAddr::new(0x1000_0000),
        PhysAddr::new(0x1000_0100),
        EntryFlags::RW,
        &mut GFA,
    )?;

    // Identity map CLINT memory
    mapper.identity_map_range(
        PhysAddr::new(0x0200_0000),
        PhysAddr::new(0x0201_0000),
        EntryFlags::RW,
        &mut GFA,
    )?;

    // Identity map SYSCON memory
    mapper.identity_map_range(
        PhysAddr::new(0x0010_0000),
        PhysAddr::new(0x0010_1000),
        EntryFlags::RW,
        &mut GFA,
    )?;

    // Map the whole physical address space to VA 0xffff_8880_0000_0000
    mapper.map(
        PHYS_MEM_OFFSET,
        PhysAddr::new(0),
        PageSize::Tb,
        EntryFlags::RWX,
        &mut GFA,
    )?;

    // Enable MMU
    Satp::write_ppn(PhysAddr::new_unchecked(rpt as *const _ as u64).page_index());
    Satp::write_mode(SatpMode::Sv48);

    // From now on, the root page table must be accessed using its virtual address
    let rpt = (PHYS_MEM_OFFSET + rpt as *const _ as usize)
        .as_mut_ptr::<PageTable>()
        .as_mut::<'static>()
        .unwrap();

    // Return the actual offset mapper
    Ok(OffsetPageMapper::new(rpt, PHYS_MEM_OFFSET))
}

/// Maps the heap allocator's virtual pages to physical memory.
///
/// # Safety
///
/// The caller must guarantee that the physical memory being mapped isn't already in use by the
/// system.
unsafe fn setup_heap(
    mapper: &mut OffsetPageMapper,
    start: VirtAddr,
    end: VirtAddr,
    phys_base: PhysAddr,
) -> Result<(), MapError> {
    let page_size = PAGE_SIZE as usize;
    let num_heap_pages = (end - start).data() / page_size;

    // Map heap pages
    for i in 0..num_heap_pages {
        let vaddr = start + i * page_size;
        let paddr = phys_base + (i * page_size) as u64;

        mapper.map(vaddr, paddr, PageSize::Kb, EntryFlags::RWX, &mut GFA)?;
    }

    Ok(())
}
