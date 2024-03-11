//! RISC-V specific memory management.

use core::ptr::addr_of_mut;

use crate::arch::riscv::{
    addr::PAGE_SIZE,
    mmu::{self, EntryFlags, PageSize, PageTable},
    PhysAddr, VirtAddr,
};
use crate::mm::allocator::BumpAllocator;
use crate::mm::{allocator::FrameAllocator, Align};
use mmu::OffsetPageMapper;

/// Base address for the physical address space.
/// TODO: this should be extracted from the device tree.
pub const PHYS_MEM_OFFSET: PhysAddr = PhysAddr::new_truncated(0x8000_0000);

/// Size of the physical memory in bytes
/// TODO: this should be extracted from the device tree.
pub const PHYS_MEM_SIZE: u64 = 32 * 1024 * 1024;

/// Virtual address at which physical memory is mapped.
pub const PHYS_TO_VIRT_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffc0_0000_0000);

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

/// Global heap allocator.
/// TODO: remove hard-coded constants.
#[global_allocator]
static HEAP: BumpAllocator =
    BumpAllocator::new(0xffff_ffc0_0100_0000, 0xffff_ffc0_0100_0000 + HEAP_MEM_SIZE);

/// Finishes up memory initialization, by setting up frame and heap allocators.
///
/// This function is called with MMU enabled after [`setup_early_vm`], so no physical addresses
/// can be dereferenced or accessed here. `rpt_va` is the virtual address of the root page table
/// set up during [`setup_early_vm`], and can be used to prepare an offset page mapper.
pub fn setup_late(rpt_va: VirtAddr) {
    // SAFETY: all these symbols are populated by the linker script
    let (text_start, text_end, rodata_start, rodata_end, data_start, data_end) = unsafe {
        (
            VirtAddr::new_unchecked(&_stext as *const _ as usize),
            VirtAddr::new_unchecked(&_etext as *const _ as usize),
            VirtAddr::new_unchecked(&_srodata as *const _ as usize),
            VirtAddr::new_unchecked(&_erodata as *const _ as usize),
            VirtAddr::new_unchecked(&_sdata as *const _ as usize),
            VirtAddr::new_unchecked(&_edata as *const _ as usize),
        )
    };

    kprintln!("Kernel memory map:");
    kprintln!("  [{} - {}] .text", text_start, text_end);
    kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
    kprintln!("  [{} - {}] .data", data_start, data_end);

    // Set up the offset page mapper that will be used to access physical memory
    // SAFETY: `rpt_va` is pointing to a valid root page table.
    let mapper = unsafe {
        OffsetPageMapper::new(&mut *(rpt_va.data() as *mut PageTable), PHYS_TO_VIRT_OFFSET)
    };

    // SAFETY: `mapper.page_table()` is the root page directory
    unsafe { mmu::dump_root_page_table(mapper.page_table()) };
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

/// Sets up early virtual memory mappings in order to relocate in startup code.
///
/// For simplicity, the whole physical memory is mapped starting at LOAD_OFFSET, and the kernel
/// is relocated to use this mapping.
///
/// This function is called from head.S with MMU off. In order for this to work, the function must
/// use PC-relative addressing for accessing kernel symbol, which is guaranteed by the Rust
/// compiler by using LLVM's `medium` mcmodel for RISC-V.
/// Moreover, at this point no frame allocator has been setup yet, so we use a few statically
/// allocated frames to perform the mappings.
///
/// The function returns the physical addresses of the kernel root page table. This page table
/// is guaranteed to be valid for the whole lifetime of the kernel.
#[no_mangle]
unsafe extern "C" fn setup_early_vm() -> u64 {
    // Use huge pages to map the physical memory for efficiency
    const PAGE_SZ: PageSize = PageSize::Gb;

    // Statically reserve a few pages for physical memory mapping.
    // Since we are using huge pages, we only need a single huge PTE and one page table.
    let mut early_allocator = {
        static EARLY_FRAME_MEM: [PageTable; 1] = [PageTable::new(); 1];
        EarlyAllocator::new(&EARLY_FRAME_MEM)
    };

    let load_pa = PHYS_MEM_OFFSET;
    let load_sz = PHYS_MEM_SIZE;
    let n_pages = load_sz.div_ceil(PAGE_SZ.size());

    // Make sure that both the virtual and physical addresses are aligned to the page size
    assert!(load_pa.is_aligned(PAGE_SZ.size()));
    assert!(PHYS_TO_VIRT_OFFSET.is_aligned(PAGE_SZ.size() as usize));

    // Statically allocat a root PTE.
    // SAFETY: kernel_rpt is the only mutable reference to KERNEL_RPT.
    let kernel_rpt = unsafe {
        static mut KERNEL_RPT: PageTable = PageTable::new();
        &mut *addr_of_mut!(KERNEL_RPT)
    };

    // Since MMU is off right now, we can use an OffsetPageMapper with no offset.
    // SAFETY: kernel_rpt points to a valid page table
    let mut kernel_mapper = unsafe { OffsetPageMapper::new(kernel_rpt, VirtAddr::new(0)) };

    // Map the whole physical memory in the virtual address space.
    // SAFETY: these mappings are unique since they are the only one existing at this point
    unsafe {
        for i in 0..n_pages {
            kernel_mapper
                .map(
                    PHYS_TO_VIRT_OFFSET + (i * PAGE_SZ.size()) as usize,
                    load_pa + i * PAGE_SZ.size(),
                    PAGE_SZ,
                    EntryFlags::KERNEL,
                    &mut early_allocator,
                )
                .unwrap();
        }
    }

    kernel_rpt as *const _ as u64
}
