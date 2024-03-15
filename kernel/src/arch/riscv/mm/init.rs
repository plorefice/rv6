//! RISC-V early virtual memory setup

use core::ptr::addr_of_mut;

use crate::{
    arch::{
        mm::{LOAD_OFFSET, PHYS_MEM_OFFSET, PHYS_MEM_SIZE, PHYS_TO_VIRT_OFFSET},
        mmu::{EntryFlags, PageSize, PageTable},
        PhysAddr, VirtAddr,
    },
    mm::Align,
};

/// Sets up early virtual memory mappings in order to relocate in startup code.
///
/// This function will map the kernel text and data in the virtual address space at [`LOAD_OFFSET`],
/// along with the complete physical memory region at [`PHYS_TO_VIRT_OFFSET`].
///
/// The function returns the physical addresses of the kernel root page table. This page table
/// is guaranteed to be valid for the whole lifetime of the kernel.
///
/// # Notes
///
/// This function is called from head.S with MMU off. In order for this to work, the function must
/// use PC-relative addressing for accessing kernel symbol, which is guaranteed by the Rust
/// compiler by using LLVM's `medium` mcmodel for RISC-V.
///
/// Moreover, at this point no frame allocator has been setup yet, so we use a few statically
/// allocated frames to perform the mappings.
#[no_mangle]
unsafe extern "C" fn setup_early_vm() -> u64 {
    extern "C" {
        fn _start();
        fn _end();
    }

    // Use 2 MiB pages, since huge pages do not typically meet alignment requirements
    const MAPPING_SIZE: PageSize = PageSize::Mb;

    // Build a page table allocator using the bottom of the physical memory.
    // SAFETY: the bottom of the physical memory is unused
    let mut l1_page_allocator = unsafe {
        EarlyPageTableAllocator::<128>::new(
            (PHYS_MEM_OFFSET + PHYS_MEM_SIZE).data() as *mut PageTable
        )
    };

    // Statically allocate a root PTE.
    // SAFETY: kernel_rpt is the only mutable reference to KERNEL_RPT.
    let kernel_rpt = unsafe {
        static mut KERNEL_RPT: PageTable = PageTable::new();
        &mut *addr_of_mut!(KERNEL_RPT)
    };

    // Early mapping function
    let mut early_map_range = |pa: PhysAddr, va: VirtAddr, size: u64| {
        let n_pages = size.div_ceil(MAPPING_SIZE.size());

        // Make sure that both the virtual and physical addresses are aligned to the page size
        assert!(pa.is_aligned(MAPPING_SIZE.size()));
        assert!(va.is_aligned(MAPPING_SIZE.size() as usize));

        // SAFETY: these mappings are unique since they are the only one existing at this point
        unsafe {
            for i in 0..n_pages {
                let offset = i * MAPPING_SIZE.size();

                create_l1_page_mapping(
                    kernel_rpt,
                    va + offset as usize,
                    pa + offset,
                    EntryFlags::KERNEL,
                    &mut l1_page_allocator,
                );
            }
        }
    };

    // Map the kernel text and data in the virtual address space
    let load_pa = PhysAddr::new(_start as *const u64 as u64);
    let load_sz = (PhysAddr::new(_end as *const u64 as u64) - load_pa).data();
    early_map_range(load_pa, LOAD_OFFSET, load_sz);

    // Temporarily map the physical memory as well, so that we can finish setup a frame allocator
    // later on and replace these early mappings
    early_map_range(PHYS_MEM_OFFSET, PHYS_TO_VIRT_OFFSET, PHYS_MEM_SIZE);

    kernel_rpt as *const _ as u64
}

unsafe fn create_l1_page_mapping<const N: usize>(
    rpt: &mut PageTable,
    va: VirtAddr,
    pa: PhysAddr,
    flags: EntryFlags,
    allocator: &mut EarlyPageTableAllocator<N>,
) {
    let l1_pte = rpt.get_entry_mut(va.vpn2()).unwrap();

    let l1_page = if !l1_pte.is_valid() {
        let l1_page = allocator.next().expect("out of early page tables");
        l1_pte.set_ppn(PhysAddr::new(l1_page as *const _ as u64).page_index());
        l1_pte.set_flags(EntryFlags::VALID);
        l1_page
    } else {
        // SAFETY: `l1_pte` is valid and thus points to a valid page table.
        //         Also, this is the only reference to it.
        unsafe { &mut *(PhysAddr::from_ppn(l1_pte.get_ppn()).data() as *mut PageTable) }
    };

    let entry = l1_page.get_entry_mut(va.vpn1()).unwrap();
    entry.set_ppn(pa.page_index());
    entry.set_flags(flags | EntryFlags::VALID);
}

/// Stack-like allocator for early page tables. Grows down from the top.
struct EarlyPageTableAllocator<const N: usize> {
    ptr: *mut PageTable,
    free: usize,
}

impl<const N: usize> EarlyPageTableAllocator<N> {
    /// Creates a new allocator for early page tables growing down from `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must be aligned and pointing to a valid memory region. There must be space
    /// for at least `N` page tables between `ptr` and the kernel end.
    pub const unsafe fn new(ptr: *mut PageTable) -> Self {
        Self { ptr, free: N }
    }

    /// Returns the next available page table.
    pub fn next(&mut self) -> Option<&'static mut PageTable> {
        if self.free == 0 {
            None
        } else {
            self.free -= 1;

            // SAFETY: `ptr` is aligned, points to a valid memory region and is initialized
            Some(unsafe {
                self.ptr = self.ptr.sub(1);
                self.ptr.write(PageTable::new());
                &mut *self.ptr
            })
        }
    }
}
