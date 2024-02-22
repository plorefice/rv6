//! RISC-V specific memory management.

use crate::arch::{
    instructions::sfence_vma,
    registers::{Sstatus, SstatusFlags},
    riscv::{
        addr::PAGE_SIZE,
        mmu::{self, EntryFlags, MapError, PageSize, PageTable},
        registers::{Satp, SatpMode},
        PhysAddr, VirtAddr,
    },
};
use crate::mm::allocator::BumpAllocator;
use crate::mm::{
    allocator::{AllocatorError, BitmapAllocator, FrameAllocator, LockedAllocator},
    Align,
};
use mmu::OffsetPageMapper;

use crate::config;

/// Base address for the physical address space
pub const PHYS_MEM_OFFSET: PhysAddr = PhysAddr::new_truncated(0x8000_0000);

/// Size of the physical memory in bytes
pub const PHYS_MEM_SIZE: u64 = 32 * 1024 * 1024;

/// Virtual memory offset at which the physical address space is mapped.
pub const PHYS_TO_VIRT_MEM_BASE: VirtAddr = VirtAddr::new_truncated({
    if cfg!(feature = "sv39") {
        0x20_0000_0000
    } else {
        0x2000_0000_0000
    }
});

/// Virtual memory address of the beginning of the kernel heap.
const HEAP_MEM_START: VirtAddr = VirtAddr::new_truncated({
    if cfg!(feature = "sv39") {
        0x10_0000_0000
    } else {
        0x1000_0000_0000
    }
});

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
    // SAFETY: __end is populated by the linker script
    let kernel_mem_end = unsafe { PhysAddr::new(&__end as *const usize as u64) };
    // TODO: parse DTB to get the memory size
    let phys_mem_end = PHYS_MEM_OFFSET + PHYS_MEM_SIZE;

    // SAFETY: no memory has yet been mapped, so these operations are inherently safe,
    //         assuming they are formally correct
    let mapper = unsafe {
        // Setup a frame allocator for physical memory outside the kernel range
        let phys_mem_start = kernel_mem_end;
        let phys_mem_size = u64::from(phys_mem_end - kernel_mem_end) - HEAP_MEM_SIZE as u64;
        setup_frame_allocator(phys_mem_start, phys_mem_size).unwrap();

        // Setup virtual memory, and switch to virtual addressing from now on.
        let mut mapper = setup_vm().expect("failed to set up virtual memory");

        // Map the frame allocator's memory (ie. the physical memory outside the kernel range)
        mapper
            .identity_map_range(
                phys_mem_start,
                phys_mem_start + phys_mem_size,
                EntryFlags::KERNEL,
            )
            .expect("failed to mmap frame allocator");

        // Allocate memory for the heap
        setup_heap(
            &mut mapper,
            HEAP_MEM_START,
            HEAP_MEM_START + HEAP_MEM_SIZE,
            phys_mem_end - HEAP_MEM_SIZE as u64,
        )
        .expect("failed to setup heap");

        mapper
    };

    // SAFETY: `mapper.page_table()` is the root page directory
    unsafe { mmu::dump_root_page_table(mapper.page_table()) };
}

/// Initializes a physical memory allocator on the specified memory range.
///
/// # Safety
///
/// The caller must guarantee that the memory being initialized isn't already in use by the system.
unsafe fn setup_frame_allocator(mem_start: PhysAddr, mem_size: u64) -> Result<(), AllocatorError> {
    let mem_start = mem_start.align_up(PAGE_SIZE);
    let mem_end = (mem_start + mem_size).align_down(PAGE_SIZE);

    // SAFETY: first initialization of a frame allocator in the system
    unsafe {
        GFA.set_allocator(BitmapAllocator::<PhysAddr, PAGE_SIZE>::init(
            mem_start, mem_end,
        )?);
    }

    Ok(())
}

/// Configures the virtual address space as expected by the kernel.
///
/// # Safety
///
/// The caller must guarantee that the physical memory being mapped isn't already in use by the
/// system.
unsafe fn setup_vm() -> Result<OffsetPageMapper<'static>, MapError> {
    // SAFETY: all these symbols are populated by the linker script
    let (text_start, text_end, rodata_start, rodata_end, data_start, data_end) = unsafe {
        (
            PhysAddr::new(&__text_start as *const usize as u64),
            PhysAddr::new(&__text_end as *const usize as u64),
            PhysAddr::new(&__rodata_start as *const usize as u64),
            PhysAddr::new(&__rodata_end as *const usize as u64),
            PhysAddr::new(&__data_start as *const usize as u64),
            PhysAddr::new(&__data_end as *const usize as u64),
        )
    };

    kprintln!("Kernel memory map:");
    kprintln!("  [{} - {}] .text", text_start, text_end);
    kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
    kprintln!("  [{} - {}] .data", data_start, data_end);

    // Allocate a frame for the root page table to be used for virtual address translation.
    // Since translation is not enabled here yet, we can use the frame's physical address directly.
    // SAFETY: the allocated frame is checked for size and properly initialized before use
    let rpt = unsafe {
        assert_eq!(PAGE_SIZE as usize, core::mem::size_of::<PageTable>());
        let rpt = GFA.alloc(1).unwrap().data() as *mut PageTable;
        rpt.write(PageTable::default());
        &mut *rpt
    };

    rpt.clear();

    // Again, since translation is off, we use an offset mapper with zero offset.
    // SAFETY: rpt has been freshly allocated and phys_offset is 0, making the mapping valid.
    let mut mapper = unsafe { OffsetPageMapper::new(rpt, VirtAddr::new(0)) };

    // SAFETY: these mappings are unique since they are the only one existing at this point
    unsafe {
        // Identity map all kernel sections
        mapper.identity_map_range(text_start, text_end, EntryFlags::KERNEL)?;
        mapper.identity_map_range(rodata_start, rodata_end, EntryFlags::KERNEL)?;
        mapper.identity_map_range(data_start, data_end, EntryFlags::KERNEL)?;

        // Identity map UART0 memory
        mapper.identity_map_range(
            PhysAddr::new(config::ns16550::BASE_ADDRESS as u64),
            PhysAddr::new((config::ns16550::BASE_ADDRESS + 0x100) as u64),
            EntryFlags::KERNEL,
        )?;

        // Identity map CLINT memory
        mapper.identity_map_range(
            PhysAddr::new(0x0200_0000),
            PhysAddr::new(0x0201_0000),
            EntryFlags::KERNEL,
        )?;

        // Identity map SYSCON memory
        mapper.identity_map_range(
            PhysAddr::new(0x0010_0000),
            PhysAddr::new(0x0010_1000),
            EntryFlags::KERNEL,
        )?;

        // Map the whole physical address space into virtual space, in order to use an offset mapper.
        // On Sv48, this could be done with a single TB entry, but on Sv39, we need four GB entries.
        #[cfg(feature = "sv39")]
        {
            for i in 0..4 {
                let offset = i * 0x4000_0000;
                mapper.map(
                    PHYS_TO_VIRT_MEM_BASE + offset,
                    PhysAddr::new(offset as u64),
                    PageSize::Gb,
                    EntryFlags::KERNEL,
                )?;
            }
        }
        #[cfg(feature = "sv48")]
        {
            mapper.map(
                PHYS_TO_VIRT_MEM_BASE,
                PhysAddr::new(0),
                PageSize::Tb,
                EntryFlags::KERNEL,
            )?;
        }
    }

    // Enable MMU
    // SAFETY: `rpt` was correctly virtual-memory-mapped above
    unsafe {
        // Allow supervisor mode to access executable and user pages
        Sstatus::set(SstatusFlags::MXR | SstatusFlags::SUM);

        Satp::write_ppn(PhysAddr::new_unchecked(rpt as *const _ as u64).page_index());

        // Memory fence: make sure the previous instruction is completed
        sfence_vma();

        #[cfg(feature = "sv39")]
        Satp::write_mode(SatpMode::Sv39);
        #[cfg(feature = "sv48")]
        Satp::write_mode(SatpMode::Sv48);

        // Flush TLB again
        sfence_vma();
    }

    // From now on, the root page table must be accessed using its virtual address
    // SAFETY: this conversion is valid because we have mapped the whole physical memory and
    //         the address of `rpt` was referred to physical memory
    let rpt = unsafe {
        let rpt = rpt as *const _ as usize;
        &mut *(PHYS_TO_VIRT_MEM_BASE + rpt).as_mut_ptr::<PageTable>()
    };

    // Return the actual offset mapper
    // SAFETY: the mapping reflects the requirements of OffsetPageMapper
    Ok(unsafe { OffsetPageMapper::new(rpt, PHYS_TO_VIRT_MEM_BASE) })
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

        // SAFETY: assuming the caller has upheld his part of the contract
        unsafe { mapper.map(vaddr, paddr, PageSize::Kb, EntryFlags::KERNEL)? };
    }

    Ok(())
}
