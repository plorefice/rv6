//! RISC-V ELF loader implementation.

use crate::{
    arch::{
        self,
        riscv::{
            addr::VirtAddrExt,
            mm::{GFA, MAPPER},
            mmu::{
                self, EntryFlags, PAGE_SIZE, PageSize, PageTable, PageTableWalker,
                dump_active_root_page_table,
            },
        },
    },
    mm::{
        addr::{Align, MemoryAddress, PhysAddr, VirtAddr},
        allocator::FrameAllocator,
    },
    proc::elf::{self, ElfLoader},
};

/// RISC-V implementation of the ArchLoader trait for loading ELF binaries into user processes.
#[derive(Debug)]
pub struct RiscvLoader {
    _private: (),
}

impl RiscvLoader {
    pub(in crate::arch::riscv) const fn new() -> Self {
        Self { _private: () }
    }
}

impl ElfLoader for RiscvLoader {
    type AddrSpace = RiscvAddrSpace;
    type Error = mmu::MapError;

    fn new_user_addr_space(&self) -> Result<Self::AddrSpace, Self::Error> {
        let mut gfa = GFA.lock();
        let gfa = gfa.as_mut().expect("GFA not initialized");

        // Let's start by getting a new root page table and its walker.
        // SAFETY: if we have correctly set up the frame allocator, this is safe
        let (mut user_mapper, user_rpt_pa) = unsafe {
            let rpt_frame = gfa.alloc(1).expect("oom");

            let rpt = rpt_frame.virt() as *mut PageTable;
            rpt.write(PageTable::new());

            (PageTableWalker::new(&mut *rpt), rpt_frame.phys())
        };

        let kernel_rpt = MAPPER.lock();
        let kernel_rpt = kernel_rpt
            .as_ref()
            .expect("MAPPER not initialized")
            .page_table();

        // Copy kernel mappings
        // SAFETY: `kernel_rpt` is valid as it is the current kernel root page table.
        unsafe {
            user_mapper.copy_kernel_mappings(kernel_rpt, gfa)?;
        }

        Ok(RiscvAddrSpace {
            rpt_pa: user_rpt_pa,
            pt_walker: user_mapper,
        })
    }

    fn choose_pie_base(
        &self,
        aspace: &mut Self::AddrSpace,
        image_min_vaddr: VirtAddr,
        image_max_vaddr: VirtAddr,
        align: usize,
        hint: usize,
    ) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn validate_user_range(
        &self,
        aspace: &Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
    ) -> Result<(), Self::Error> {
        // TODO: implement proper validation
        Ok(())
    }

    fn map_anonymous(
        &self,
        aspace: &mut Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
        flags: elf::SegmentFlags,
    ) -> Result<(), Self::Error> {
        // Ignore zero-length mappings
        if len == 0 {
            return Ok(());
        }

        assert!(vaddr.is_aligned(self.page_size()));
        assert!(len.is_aligned(self.page_size()));

        let n_pages = len / self.page_size();

        let mut gfa = GFA.lock();
        let gfa = gfa.as_mut().expect("GFA not initialized");

        // Allocate a physical frame for the mapping
        let frame = gfa.alloc(n_pages).expect("oom");

        for i in 0..n_pages {
            let va = vaddr + i * self.page_size();
            let pa = frame.phys() + i * self.page_size();

            // Map each page
            // SAFETY: caller must ensure that vaddr and len are page-aligned and valid.
            unsafe {
                aspace.pt_walker.map(
                    va,
                    pa,
                    PageSize::Kb,
                    EntryFlags::from_segment_flags(flags) | EntryFlags::USER | EntryFlags::ACCESS,
                    gfa,
                )?;
            }
        }

        Ok(())
    }

    fn protect_range(
        &self,
        aspace: &mut Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
        flags: elf::SegmentFlags,
    ) -> Result<(), Self::Error> {
        // Ignore zero-length mappings
        if len == 0 {
            return Ok(());
        }

        // Change permissions of already mapped pages
        // SAFETY: caller must ensure that vaddr and len are page-aligned and valid
        unsafe {
            aspace.pt_walker.update_mapping(
                vaddr,
                len,
                EntryFlags::from_segment_flags(flags) | EntryFlags::USER | EntryFlags::ACCESS,
            )?
        };

        Ok(())
    }

    fn copy_to_user(
        &self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: VirtAddr,
        src: &[u8],
    ) -> Result<(), Self::Error> {
        // SAFETY: caller must ensure that dst_vaddr is valid and mapped
        unsafe {
            aspace.with_addr_space(|| {
                // Copy user code into place
                // SAFETY: caller must ensure that dst_vaddr is valid and mapped
                arch::with_user_access(|| unsafe {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), dst_vaddr.as_mut_ptr(), src.len());
                });
            });
        }

        Ok(())
    }

    fn zero_user(
        &self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: VirtAddr,
        len: usize,
    ) -> Result<(), Self::Error> {
        // SAFETY: caller must ensure that dst_vaddr is valid and mapped
        unsafe {
            aspace.with_addr_space(|| {
                // Zero user data/bss
                // SAFETY: caller must ensure that dst_vaddr is valid and mapped
                arch::with_user_access(|| unsafe {
                    core::ptr::write_bytes(dst_vaddr.as_mut_ptr::<u8>(), 0, len);
                });
            });
        }

        Ok(())
    }

    fn finalize_image(
        &self,
        aspace: &mut Self::AddrSpace,
        mapped_exec_ranges: &[(VirtAddr, VirtAddr)],
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn page_size(&self) -> usize {
        PAGE_SIZE
    }
}

/// RISC-V user address space implementation for ELF loading.
pub struct RiscvAddrSpace {
    rpt_pa: PhysAddr,
    pt_walker: PageTableWalker<'static>,
}

impl RiscvAddrSpace {
    /// Returns the physical address of the root page table for this address space.
    pub fn root_page_table_pa(&self) -> PhysAddr {
        self.rpt_pa
    }

    /// Temporarily switches to this address space, runs the given closure, and then switches back.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address space is valid and properly set up before calling this function.
    pub unsafe fn with_addr_space<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // SAFETY: caller must ensure that `aspace` is valid and properly set up.
        let prev = unsafe { mmu::switch_page_table(self.rpt_pa) };

        let ret = f();

        // SAFETY: we are restoring the kernel page table, which is always valid.
        unsafe { mmu::switch_page_table(prev) };
        ret
    }
}
