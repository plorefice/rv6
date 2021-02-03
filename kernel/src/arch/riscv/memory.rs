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
use spin::Mutex;

/// Virtual memory offset at which the physical address space is mapped.
pub const PHYS_MEM_OFFSET: VirtAddr = VirtAddr::new_truncated(0x8000_0000_0000);

/// Kernel load address, as specified in linker script.
pub const LOAD_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffe0_0000_0000);

/// Virtual memory address of the beginning of the kernel heap.
const HEAP_MEM_START: VirtAddr = VirtAddr::new_truncated(0xCAFE_0000_0000);

/// Size of the heap in bytes (1 MiB)
const HEAP_MEM_SIZE: usize = 1024 * 1024;

// Defined in linker script
extern "C" {
    /// The starting word of the kernel in memory.
    static _start: usize;
    /// The ending word of the kernel in memory.
    static _end: usize;
    /// The starting word of the text section in memory.
    static _stext: usize;
    /// The ending word of the text section in memory.
    static _etext: usize;
    /// The starting word of the RO data section in memory.
    static _srodata: usize;
    /// The ending word of the RO data section in memory.
    static _erodata: usize;
    /// The starting word of the data section in memory.
    static _sdata: usize;
    /// The ending word of the data section in memory.
    static _edata: usize;
}

/// Global frame allocator (GFA).
pub static mut GFA: LockedAllocator<BitmapAllocator<PhysAddr, PAGE_SIZE>> = LockedAllocator::new();

/// Global heap allocator.
#[global_allocator]
static HEAP: BumpAllocator =
    BumpAllocator::new(HEAP_MEM_START.data(), HEAP_MEM_START.data() + HEAP_MEM_SIZE);

/// Initializes the system memory, by setting up a frame allocator and performing MMU late setup.
///
/// This function is called with MMU enabled after [`early_setup_vm`], so do not use physical
/// addresses here!
///
/// # Safety
///
/// Memory safety basically does not exist before this point :)
pub unsafe fn init() {
    let kernel_mem_end = PhysAddr::new(&_end as *const usize as u64);
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
    let text_start = PhysAddr::new(&_stext as *const usize as u64);
    let text_end = PhysAddr::new(&_etext as *const usize as u64);
    let rodata_start = PhysAddr::new(&_srodata as *const usize as u64);
    let rodata_end = PhysAddr::new(&_erodata as *const usize as u64);
    let data_start = PhysAddr::new(&_sdata as *const usize as u64);
    let data_end = PhysAddr::new(&_edata as *const usize as u64);

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

    // Map the GFA descriptor table
    // TODO: use the correct values here, as taken from the GFA descriptor
    mapper.identity_map_range(
        PhysAddr::new(0x8026a000),
        PhysAddr::new(0x8026a000 + 0x10000),
        EntryFlags::RW,
        &mut GFA,
    )?;

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
        PageSize::Gb,
        EntryFlags::RWX,
        &mut GFA,
    )?;

    // Enable MMU
    Satp::write_ppn(PhysAddr::new_unchecked(rpt as *const _ as u64).page_index());
    Satp::write_mode(SatpMode::Sv48);

    // Flush TLB
    sbi::rfence::remote_sfence_vma(0, None, 0, 0).unwrap();

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

        kprintln!("{} -> {}", vaddr, paddr);

        mapper.map(vaddr, paddr, PageSize::Kb, EntryFlags::RWX, &mut GFA)?;
    }

    Ok(())
}

struct EarlyAllocator<const N: usize> {
    frame_stack: [PageTable; N],
    next_free: usize,
}

impl<const N: usize> EarlyAllocator<N> {
    const fn new() -> Self {
        Self {
            frame_stack: [PageTable::new(); N],
            next_free: 0,
        }
    }
}

impl<const N: usize> FrameAllocator<PhysAddr, PAGE_SIZE> for EarlyAllocator<N> {
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysAddr> {
        if self.next_free + count >= N {
            None
        } else {
            let i = self.next_free;
            self.next_free += count;
            Some(PhysAddr::new(&self.frame_stack[i] as *const _ as u64))
        }
    }

    unsafe fn free(&mut self, _address: PhysAddr) {}
}

static EARLY_FRAME_ALLOCATOR: Mutex<EarlyAllocator<1024>> = Mutex::new(EarlyAllocator::new());

/// FFI-safe 2-element tuple.
///
/// The ABI guarantees that when used as a return value, its elements will be passed in a0 and a1.
#[repr(C)]
struct FfiPair {
    a0: u64,
    a1: u64,
}

/// Sets up early virtual memory mappings in order to relocate in startup code.
///
/// This function is called from head.S with MMU off. In order for this to work, the function must
/// use PC-relative addressing for accessing kernel symbol, which is guaranteed by the Rust
/// compiler by using LLVM's `medium` mcmodel for RISC-V.
/// Moreover, at this point no frame allocator has been setup yet, so we use a few statically
/// allocated pages to perform some early mappings, which will then be replaced after relocation.
///
/// The function returns two pointer-sized integers corresponding to the physical addresses of
/// the kernel and trampoline root page tables. The kernel root page table contains early mappings
/// of the whole kernel memory. The trampoline page table is used to perform the switch from
/// physical to virtual addressing.
#[no_mangle]
unsafe extern "C" fn early_setup_vm() -> FfiPair {
    extern "C" {
        static _start: usize;
        static _end: usize;
    }

    let load_pa = &_start as *const _ as u64;
    let load_sz = &_end as *const _ as u64 - load_pa;
    let page_sz = PageSize::Mb;
    let n_pages = load_sz as usize / page_sz.size();

    let kernel_rpt_pa = EARLY_FRAME_ALLOCATOR.lock().alloc(1).unwrap();
    let trampoline_rpt_pa = EARLY_FRAME_ALLOCATOR.lock().alloc(1).unwrap();

    let kernel_rpt = (kernel_rpt_pa.data() as *mut PageTable).as_mut().unwrap();
    let trampoline_rpt = (trampoline_rpt_pa.data() as *mut PageTable)
        .as_mut()
        .unwrap();

    let mut kernel_mapper = OffsetPageMapper::new(kernel_rpt, VirtAddr::new(0));
    let mut trampoline_mapper = OffsetPageMapper::new(trampoline_rpt, VirtAddr::new(0));

    // Map the whole kernel in the virtual address space
    for i in 0..n_pages {
        kernel_mapper
            .map(
                LOAD_OFFSET + i * page_sz.size(),
                PhysAddr::new(load_pa) + (i * page_sz.size()) as u64,
                page_sz,
                EntryFlags::RWX | EntryFlags::ACCESS | EntryFlags::DIRTY,
                &mut *EARLY_FRAME_ALLOCATOR.lock(),
            )
            .unwrap();
    }

    // We only need to map a single mega-page in the trampoline, since it is only used briefly
    // in head code to jump to virtual addressing
    trampoline_mapper
        .map(
            LOAD_OFFSET,
            PhysAddr::new(load_pa),
            page_sz,
            EntryFlags::RWX | EntryFlags::ACCESS | EntryFlags::DIRTY,
            &mut *EARLY_FRAME_ALLOCATOR.lock(),
        )
        .unwrap();

    FfiPair {
        a0: kernel_rpt_pa.data(),
        a1: trampoline_rpt_pa.data(),
    }
}
