//! RISC-V specific memory management.

use core::{
    alloc::{GlobalAlloc, Layout},
    mem::{self, size_of, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    arch::riscv::{
        addr::{PhysAddr, VirtAddr, PAGE_SIZE},
        instructions::sfence_vma,
        mmu::{self, EntryFlags, PageSize, PageTable},
        registers::Satp,
    },
    mm::{
        allocator::{BumpAllocator, BumpFrameAllocator, FrameAllocator},
        Align, PhysicalAddress,
    },
    proc::{Process, ProcessMemory},
};
use fdt::{Fdt, PropEncodedArray};
use mmu::PageTableWalker;
use spin::Mutex;

mod init;
pub mod proc;

/// Base address for the physical address space.
pub static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Virtual offset at which physical memory is mapped.
pub const PHYS_TO_VIRT_OFFSET: VirtAddr = VirtAddr::new_truncated(0x20_0000_0000);

/// Base address for the kernel heap.
const HEAP_MEM_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffc0_0000_0000);

/// Base address for the memory-mapper I/O region.
const IOMAP_MEM_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffe0_0000_0000);

/// Virtual address at which the kernel is loaded.
const LOAD_OFFSET: VirtAddr = VirtAddr::new_truncated(0xffff_ffff_8000_0000);

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

/// Global frame allocator.
static GFA: Mutex<Option<BumpFrameAllocator<PAGE_SIZE, PhysAddr>>> = Mutex::new(None);

/// Global heap allocator.
/// TODO: remove hard-coded constants.
#[global_allocator]
static HEAP: BumpAllocator = BumpAllocator::new(HEAP_MEM_OFFSET.data(), IOMAP_MEM_OFFSET.data());

/// I/O virtual memory allocator.
static IOMAP: BumpAllocator = BumpAllocator::new(IOMAP_MEM_OFFSET.data(), LOAD_OFFSET.data());

/// Kernel global page mapper.
static MAPPER: Mutex<Option<PageTableWalker<'static>>> = Mutex::new(None);

/// Finishes up memory initialization, by setting up frame and heap allocators.
///
/// This function is called with MMU enabled after [`setup_early_vm`], so no physical addresses
/// can be dereferenced or accessed here. `rpt_va` is the virtual address of the root page table
/// set up during [`setup_early_vm`], and can be used to prepare an offset page mapper.
pub fn setup_late(fdt: &Fdt, early_rpt: VirtAddr) {
    // SAFETY: all these symbols are populated by the linker script
    let (
        kernel_start,
        text_start,
        text_end,
        rodata_start,
        rodata_end,
        data_start,
        data_end,
        kernel_end,
    ) = unsafe {
        (
            VirtAddr::new(&_start as *const _ as usize),
            VirtAddr::new(&_stext as *const _ as usize),
            VirtAddr::new(&_etext as *const _ as usize),
            VirtAddr::new(&_srodata as *const _ as usize),
            VirtAddr::new(&_erodata as *const _ as usize),
            VirtAddr::new(&_sdata as *const _ as usize),
            VirtAddr::new(&_edata as *const _ as usize),
            VirtAddr::new(&_end as *const _ as usize),
        )
    };

    kprintln!("Kernel memory map:");
    kprintln!("  [{} - {}] .text", text_start, text_end);
    kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
    kprintln!("  [{} - {}] .data", data_start, data_end);

    // Set up the same page mapper used for early mappings, which allows use to convert
    // kernel addresses.
    // TODO: the better way would be to simply compute everything statically in setup_early_vm
    // and pass these around, to avoid this mapper.
    // SAFETY: `rpt_va` is pointing to a valid root page table.
    let early_kernel_mapper =
        unsafe { PageTableWalker::new(&mut *(early_rpt.as_mut_ptr::<PageTable>())) };

    // Extract memory map from the FDT
    let mem_region = fdt.find(|n| n.name() == "memory").unwrap().unwrap();
    let (mem_base, mem_size) = mem_region
        .property::<PropEncodedArray<(u64, u64)>>("reg")
        .unwrap()
        .next()
        .unwrap();

    // Save the base address of the physical memory for quicker translations
    PHYS_MEM_OFFSET.store(mem_base, Ordering::Relaxed);
    let mem_base = PhysAddr::new(mem_base);

    // Set up a frame allocator for the unused physical memory
    setup_frame_allocator(&early_kernel_mapper, mem_base, mem_size);

    // Now that we have a proper frame allocator, we can replace the early mappings with page
    // mappings that use properly tracked frames
    let mut gfa = GFA.lock();
    let gfa = gfa.as_mut().unwrap();

    // Let's start by getting a new root page table and its walker.
    // SAFETY: if we have correctly set up the frame allocator, this is safe
    let (mut mapper, rpt_pa) = unsafe {
        let rpt_frame = gfa.alloc(1).unwrap();

        let rpt = rpt_frame.ptr as *mut PageTable;
        rpt.write(PageTable::new());

        (PageTableWalker::new(&mut *rpt), rpt_frame.paddr)
    };

    // SAFETY: new mapper
    unsafe {
        // Remap the kernel
        let kern_start_pa = early_kernel_mapper.virt_to_phys(kernel_start).unwrap();
        let kern_end_pa = early_kernel_mapper.virt_to_phys(kernel_end).unwrap();

        mapper
            .map_range(
                LOAD_OFFSET,
                kern_start_pa..kern_end_pa,
                PageSize::Kb,
                EntryFlags::KERNEL,
                gfa,
            )
            .unwrap();

        // Remap the whole physical memory
        mapper
            .map_range(
                PHYS_TO_VIRT_OFFSET,
                mem_base..(mem_base + mem_size),
                PageSize::Mb,
                EntryFlags::KERNEL,
                gfa,
            )
            .unwrap();
    }

    // Preallocate and map some memory for the heap
    // TODO: frames should be allocated on demand, rather than ahead of time
    const HEAP_PREALLOC_SIZE: usize = 1024 * 1024;

    let map_size = PageSize::Kb;
    assert_eq!(HEAP_PREALLOC_SIZE % map_size.size() as usize, 0);

    let heap_prealloc_base = IOMAP_MEM_OFFSET - HEAP_PREALLOC_SIZE;
    let n_pages = HEAP_PREALLOC_SIZE / map_size.size() as usize;

    // SAFETY: n_pages is enough to cover the requested heap size
    let frame = unsafe { gfa.alloc(n_pages).expect("oom for heap allocation") };

    // SAFETY: new mapper
    unsafe {
        mapper
            .map_range(
                heap_prealloc_base,
                frame.paddr..frame.paddr + HEAP_PREALLOC_SIZE as u64,
                map_size,
                EntryFlags::KERNEL,
                gfa,
            )
            .unwrap();
    }

    // Swap page tables
    // SAFETY: Jesus take the wheel!
    unsafe {
        Satp::write_ppn(rpt_pa.page_index());
        sfence_vma();
    }

    // SAFETY: `mapper.page_table()` is the root page directory
    unsafe {
        kprintln!("Active memory mappings:");
        mmu::dump_root_page_table(mapper.page_table())
    };

    // Everything went well, configure this mapper as global
    *MAPPER.lock() = Some(mapper);
}

fn setup_frame_allocator(ptw: &PageTableWalker, base: PhysAddr, len: u64) {
    // SAFETY: populated by the linker script
    let kernel_end = unsafe { VirtAddr::new(&_end as *const _ as usize) };

    let virt_base = kernel_end.align_up(PAGE_SIZE as usize);
    let phys_base = ptw.virt_to_phys(virt_base).unwrap();
    let phys_end = base + len;

    kprintln!("Available physical memory:");
    kprintln!("  [{phys_base:016x} - {phys_end:016x}]");

    // SAFETY: `phys_base` and `phys_end` are valid physical addresses
    *GFA.lock() = Some(unsafe { BumpFrameAllocator::new(phys_base, phys_end) });
}

/// Translates a PA into the corresponding VA.
///
/// The translation assumes that physical memory is fully mapped at `PHYS_TO_MEM_OFFSET`.
///
/// # Safety
///
/// For performance reasons, no checks are performed on `pa`. It is assumed that the caller
/// upholds the condition `phys_mem_start <= pa < phys_mem_end`.
pub unsafe fn pa_to_va(pa: impl PhysicalAddress<u64>) -> VirtAddr {
    PHYS_TO_VIRT_OFFSET + (pa.into() - PHYS_MEM_OFFSET.load(Ordering::Relaxed)) as usize
}

/// Maps the physical IO region `base..base+len` to virtual memory and returns its address.
///
/// # Safety
///
/// See various `map` functions.
pub unsafe fn iomap(base: impl PhysicalAddress<u64>, len: u64) -> *mut u8 {
    // iomap entire pages only
    let len = len.align_up(PAGE_SIZE);

    // TODO: should we take an alignment requirement from the caller?
    let layout = Layout::from_size_align(len as usize, mem::align_of::<u64>())
        .expect("invalid memory layout");

    // SAFETY: the layout is valid
    let ptr = unsafe { IOMAP.alloc(layout) };

    let start = PhysAddr::new(base.into());
    let end = start + len;

    // SAFETY: all checks are in place
    unsafe {
        MAPPER
            .lock()
            .as_mut()
            .expect("no mapper?")
            .map_range(
                VirtAddr::new(ptr as usize),
                start..end,
                PageSize::Kb,
                EntryFlags::MMIO,
                GFA.lock().as_mut().unwrap(),
            )
            .unwrap();
    }

    ptr
}

/// A handle to an object allocated in physical memory.
// TODO: implement Drop
pub struct Cookie<T: ?Sized> {
    ptr: NonNull<T>,
    phys: PhysAddr,
}

impl<T: ?Sized> Cookie<T> {
    /// Returns the physical address of the allocated object.
    pub fn phys_addr(&self) -> impl PhysicalAddress<u64> {
        self.phys
    }
}

impl<T: ?Sized> AsRef<T> for Cookie<T> {
    fn as_ref(&self) -> &T {
        // SAFETY: Cookie is safe by construction
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ?Sized> AsMut<T> for Cookie<T> {
    fn as_mut(&mut self) -> &mut T {
        // SAFETY: Cookie is safe by construction
        unsafe { self.ptr.as_mut() }
    }
}

impl<T: ?Sized> Deref for Cookie<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T: ?Sized> DerefMut for Cookie<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

/// Allocates an object of type `T` in contiguous physical memory, initializing with the contents
/// of `val` following the same semantics of [`core::ptr::write`].
///
/// The cookie tracks the location of the object in physical memory.
pub fn palloc<T: Sized>(val: T) -> Cookie<T> {
    let layout = Layout::new::<T>();

    // SAFETY: `T` is a sized type
    let frame = unsafe {
        GFA.lock()
            .as_mut()
            .unwrap()
            .alloc(layout.size())
            .expect("oom")
    };

    // Initialize ptr with the contents of `val`
    // SAFETY: `frame.ptr` is known to be a valid pointer
    let ptr = unsafe {
        let ptr = NonNull::new_unchecked(frame.ptr as *mut T);
        ptr.as_ptr().write(val);
        ptr
    };

    Cookie {
        ptr,
        phys: frame.paddr,
    }
}

/// Allocates a block of contiguous physical memory, mapped into contiguous virtual memory,
/// according to the provided layout.
///
/// # Safety
///
/// `layout` must have non-zero size. The returned memory is unitialized.
pub unsafe fn alloc_contiguous(layout: Layout) -> Cookie<[u8]> {
    assert!(layout.align() <= PAGE_SIZE as usize);

    // SAFETY: assuming caller has upheld safety contract
    let frame = unsafe {
        GFA.lock()
            .as_mut()
            .unwrap()
            .alloc(layout.size())
            .expect("oom")
    };

    // SAFETY: we have just allocated `layout.size()` bytes into `frame`
    let slice = unsafe { slice::from_raw_parts_mut(frame.ptr as *mut u8, layout.size()) };

    Cookie {
        ptr: NonNull::new(slice).unwrap(),
        phys: frame.paddr,
    }
}

/// Same as [`alloc_contiguous`], but initializes the memory with zeros.
///
/// # Safety
///
/// See [`alloc_contiguous`].
pub unsafe fn alloc_contiguous_zeroed(layout: Layout) -> Cookie<[u8]> {
    // SAFETY: assuming caller has upheld safety contract
    unsafe {
        let mut ck = alloc_contiguous(layout);
        ck.as_mut().fill(0);
        ck
    }
}
