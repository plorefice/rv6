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
    allocator::{AllocatorError, BitmapAllocator, FrameAllocator},
    Align,
};
use mmu::OffsetPageMapper;
use spin::Mutex;

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

/// Kernel load address, as specified in linker script.
pub const LOAD_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffff_8000_0000);

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
pub static GFA: Mutex<Option<BitmapAllocator<PhysAddr, PAGE_SIZE>>> = Mutex::new(None);

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
    // SAFETY: _end is populated by the linker script
    let kernel_mem_end = unsafe { PhysAddr::new(&_end as *const usize as u64) };
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
                GFA.lock().as_mut().unwrap(),
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
        *GFA.lock() = Some(BitmapAllocator::<PhysAddr, PAGE_SIZE>::init(
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
            PhysAddr::new(&_stext as *const usize as u64),
            PhysAddr::new(&_etext as *const usize as u64),
            PhysAddr::new(&_srodata as *const usize as u64),
            PhysAddr::new(&_erodata as *const usize as u64),
            PhysAddr::new(&_sdata as *const usize as u64),
            PhysAddr::new(&_edata as *const usize as u64),
        )
    };

    kprintln!("Kernel memory map:");
    kprintln!("  [{} - {}] .text", text_start, text_end);
    kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
    kprintln!("  [{} - {}] .data", data_start, data_end);

    let mut gfa = GFA.lock();
    let gfa = gfa
        .as_mut()
        .expect("call to setup_vm() without a valid frame allocator");

    // Allocate a frame for the root page table to be used for virtual address translation.
    // Since translation is not enabled here yet, we can use the frame's physical address directly.
    // SAFETY: the allocated frame is checked for size and properly initialized before use
    let rpt = unsafe {
        assert_eq!(PAGE_SIZE as usize, core::mem::size_of::<PageTable>());
        let rpt = gfa.alloc(1).unwrap().data() as *mut PageTable;
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
        mapper.identity_map_range(text_start, text_end, EntryFlags::KERNEL, gfa)?;
        mapper.identity_map_range(rodata_start, rodata_end, EntryFlags::KERNEL, gfa)?;
        mapper.identity_map_range(data_start, data_end, EntryFlags::KERNEL, gfa)?;

        // Identity map UART0 memory
        mapper.identity_map_range(
            PhysAddr::new(config::ns16550::BASE_ADDRESS as u64),
            PhysAddr::new((config::ns16550::BASE_ADDRESS + 0x100) as u64),
            EntryFlags::KERNEL,
            gfa,
        )?;

        // Identity map CLINT memory
        mapper.identity_map_range(
            PhysAddr::new(0x0200_0000),
            PhysAddr::new(0x0201_0000),
            EntryFlags::KERNEL,
            gfa,
        )?;

        // Identity map SYSCON memory
        mapper.identity_map_range(
            PhysAddr::new(0x0010_0000),
            PhysAddr::new(0x0010_1000),
            EntryFlags::KERNEL,
            gfa,
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
                    gfa,
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

    let mut gfa = GFA.lock();
    let gfa = gfa.as_mut().unwrap();

    // Map heap pages
    for i in 0..num_heap_pages {
        let vaddr = start + i * page_size;
        let paddr = phys_base + (i * page_size) as u64;

        // SAFETY: assuming the caller has upheld his part of the contract
        unsafe { mapper.map(vaddr, paddr, PageSize::Kb, EntryFlags::KERNEL, gfa)? };
    }

    Ok(())
}

struct EarlyAllocator<const N: usize> {
    frame_stack: &'static [PageTable; N],
    next_free: usize,
}

impl<const N: usize> EarlyAllocator<N> {
    const fn new(mem: &'static [PageTable; N]) -> Self {
        Self {
            frame_stack: mem,
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
        fn _start();
        fn _end();
    }

    // Statically reserve a few pages for early mappings
    let mut early_allocator = {
        static EARLY_FRAME_MEM: [PageTable; 128] = [PageTable::new(); 128];
        EarlyAllocator::new(&EARLY_FRAME_MEM)
    };

    let load_pa = _start as *const core::ffi::c_void as u64;
    let load_sz = _end as *const core::ffi::c_void as u64 - load_pa;
    let page_sz = PageSize::Mb;
    let n_pages = load_sz.div_ceil(page_sz.size());

    // SAFETY: early_allocator uses local, statically allocated memory
    let kernel_rpt_pa = unsafe { early_allocator.alloc(1).unwrap() };
    // SAFETY: early_allocator uses local, statically allocated memory
    let trampoline_rpt_pa = unsafe { early_allocator.alloc(1).unwrap() };

    // SAFETY: EarlyAllocator yields page-aligned addresses with initialized data
    let kernel_rpt = unsafe { &mut *(kernel_rpt_pa.data() as *mut PageTable) };
    // SAFETY: EarlyAllocator yields page-aligned addresses with initialized data
    let trampoline_rpt = unsafe { &mut *(trampoline_rpt_pa.data() as *mut PageTable) };

    // SAFETY: kernel_rpt points to a valid page table
    let mut kernel_mapper = unsafe { OffsetPageMapper::new(kernel_rpt, VirtAddr::new(0)) };
    // SAFETY: trampoline_rpt points to a valid page table
    let mut trampoline_mapper = unsafe { OffsetPageMapper::new(trampoline_rpt, VirtAddr::new(0)) };

    // Map the whole kernel in the virtual address space
    // SAFETY: these mappings are unique since they are the only one existing at this point
    unsafe {
        for i in 0..n_pages {
            kernel_mapper
                .map(
                    LOAD_OFFSET + (i * page_sz.size()) as usize,
                    PhysAddr::new(load_pa) + i * page_sz.size(),
                    page_sz,
                    EntryFlags::KERNEL,
                    &mut early_allocator,
                )
                .unwrap();
        }
    }

    // We only need to map a single mega-page in the trampoline, since it is only used briefly
    // in head code to jump to virtual addressing
    // SAFETY: these mappings are unique since they are the only one existing at this point
    unsafe {
        trampoline_mapper
            .map(
                LOAD_OFFSET,
                PhysAddr::new(load_pa),
                page_sz,
                EntryFlags::KERNEL,
                &mut early_allocator,
            )
            .unwrap();
    }

    FfiPair {
        a0: kernel_rpt_pa.data(),
        a1: trampoline_rpt_pa.data(),
    }
}
