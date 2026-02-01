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
};
use fdt::{Fdt, PropEncodedArray};
use mmu::PageTableWalker;
use spin::Mutex;

mod init;

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
    unsafe { mmu::dump_root_page_table(mapper.page_table()) };

    // Everything went well, configure this mapper as global
    *MAPPER.lock() = Some(mapper);
}

// TODO: remove me, this is just a sample
pub(crate) fn spawn_test_userspace_process() {
    const USER_TEXT_VA: VirtAddr = VirtAddr::new_truncated(0x4000_0000);
    const USER_DATA_VA: VirtAddr = VirtAddr::new_truncated(0x4000_1000);
    const USER_STACK_VA: VirtAddr = VirtAddr::new_truncated(0x4000_2000);

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
        // Copy kernel mappings
        user_mapper
            .copy_kernel_mappings(MAPPER.lock().as_ref().unwrap().page_table(), gfa)
            .unwrap();

        // Map user code pages
        user_mapper
            .map(
                USER_TEXT_VA,
                code_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RX,
                gfa,
            )
            .unwrap();

        // Map user data pages
        user_mapper
            .map(
                USER_DATA_VA,
                data_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RW,
                gfa,
            )
            .unwrap();

        // Map user stack pages
        user_mapper
            .map(
                USER_STACK_VA,
                stack_frame.paddr,
                PageSize::Kb,
                EntryFlags::USER_RW,
                gfa,
            )
            .unwrap();
    }

    // Swap page tables.
    // SAFETY: everything is dandy since the kernel mappings were copied over.
    unsafe {
        Satp::write_ppn(user_rpt_pa.page_index());
        sfence_vma();
    }

    // SAFETY: `code_frame` was just allocated and mapped.
    unsafe {
        // addi a0, zero, 1
        // ecall
        let code: [u8; 8] = [0x13, 0x05, 0x10, 0x00, 0x73, 0x00, 0x00, 0x00];

        core::ptr::copy_nonoverlapping(
            code.as_ptr(),
            pa_to_va(code_frame.paddr).as_mut_ptr(),
            code.len(),
        );

        // Ensure instruction cache is up to date
        crate::arch::riscv::instructions::fence_i();
    }

    // Configure sepc and sstatus for user mode
    // SAFETY: `USER_TEXT_VA` is properly mapped.
    unsafe {
        use crate::arch::riscv::registers::{Sepc, Sstatus, SstatusFlags};

        Sepc::write(USER_TEXT_VA.data() as u64);
        Sstatus::update(|f| {
            f.remove(SstatusFlags::SPP); // Set to user mode
            f.insert(SstatusFlags::SPIE); // Enable interrupts on return to user mode
        });
    }

    kprintln!("Switching to userspace...");

    // Switch to user stack and jump to user mode
    // NOTE: stack swap and sret must be "atomic": no stack usage must happen in between!
    // SAFETY: everything is properly set up for user mode.
    unsafe {
        let user_sp = (USER_STACK_VA + PAGE_SIZE as usize).data();

        core::arch::asm!(
            // sscrath = kernel sp
            "csrw sscratch, sp",
            // sp = user sp
            "mv sp, {user_sp}",
            // sret to user mode
            "sret",
            user_sp = in(reg) user_sp,
            options(noreturn));
    }

    // We should never return here!
    kprintln!("Execution returned to kernel mode unexpectedly!");
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

// TODO: implement Drop
pub struct Cookie<T: ?Sized> {
    ptr: NonNull<T>,
    phys: PhysAddr,
}

impl<T: ?Sized> Cookie<T> {
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
