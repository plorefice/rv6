//! RISC-V ELF loader implementation.

use crate::{
    arch::{
        self, PAGE_SIZE,
        riscv::{
            addr::{PhysAddr, VirtAddr},
            mm::{GFA, MAPPER},
            mmu::{
                self, EntryFlags, PageSize, PageTable, PageTableWalker, dump_active_root_page_table,
            },
        },
    },
    mm::{Align, allocator::FrameAllocator},
    proc::{
        Process, ProcessMemory,
        elf::{self, ArchLoader},
    },
};

/// RISC-V implementation of the ArchLoader trait for loading ELF binaries into user processes.
#[derive(Default, Debug)]
pub struct RiscvLoader {
    pub(crate) rpt_pa: u64, // Physical address of the root page table
}

impl ArchLoader for RiscvLoader {
    type AddrSpace = PageTableWalker<'static>;
    type Error = mmu::MapError;

    fn new_user_addr_space(&mut self) -> Result<Self::AddrSpace, Self::Error> {
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

        self.rpt_pa = user_rpt_pa.data();

        Ok(user_mapper)
    }

    fn choose_pie_base(
        &mut self,
        aspace: &mut Self::AddrSpace,
        image_min_vaddr: usize,
        image_max_vaddr: usize,
        align: usize,
        hint: usize,
    ) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn validate_user_range(
        &self,
        aspace: &Self::AddrSpace,
        vaddr: usize,
        len: usize,
    ) -> Result<(), Self::Error> {
        // TODO: implement proper validation
        Ok(())
    }

    fn map_anonymous(
        &mut self,
        aspace: &mut Self::AddrSpace,
        vaddr: usize,
        len: usize,
        flags: elf::SegmentFlags,
    ) -> Result<(), Self::Error> {
        // Ignore zero-length mappings
        if len == 0 {
            return Ok(());
        }

        assert!(vaddr.is_aligned(self.page_size()));
        assert!(len.is_aligned(self.page_size()));

        let n_pages = len / (self.page_size() as usize);

        let mut gfa = GFA.lock();
        let gfa = gfa.as_mut().expect("GFA not initialized");

        // Allocate a physical frame for the mapping
        let frame = gfa.alloc(n_pages).expect("oom");

        for i in 0..n_pages {
            let va = vaddr + i * (self.page_size() as usize);
            let pa = frame.phys() + (i as u64) * self.page_size();

            // Map each page
            // SAFETY: caller must ensure that vaddr and len are page-aligned and valid.
            unsafe {
                aspace.map(
                    va.into(),
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
        &mut self,
        aspace: &mut Self::AddrSpace,
        vaddr: usize,
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
            aspace.update_mapping(
                vaddr.into(),
                len,
                EntryFlags::from_segment_flags(flags) | EntryFlags::USER | EntryFlags::ACCESS,
            )?
        };

        Ok(())
    }

    fn copy_to_user(
        &mut self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: usize,
        src: &[u8],
    ) -> Result<(), Self::Error> {
        self.with_addr_space(aspace, || {
            // Copy user code into place
            // SAFETY: caller must ensure that dst_vaddr is valid and mapped
            arch::with_user_access(|| unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    VirtAddr::new(dst_vaddr).as_mut_ptr(),
                    src.len(),
                );
            });
        });

        Ok(())
    }

    fn zero_user(
        &mut self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: usize,
        len: usize,
    ) -> Result<(), Self::Error> {
        self.with_addr_space(aspace, || {
            // Zero user data/bss
            // SAFETY: caller must ensure that dst_vaddr is valid and mapped
            arch::with_user_access(|| unsafe {
                core::ptr::write_bytes(VirtAddr::new(dst_vaddr).as_mut_ptr::<u8>(), 0, len);
            });
        });

        Ok(())
    }

    fn finalize_image(
        &mut self,
        aspace: &mut Self::AddrSpace,
        mapped_exec_ranges: &[(usize, usize)],
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn with_addr_space<F, R>(
        &mut self,
        aspace: &mut Self::AddrSpace,
        f: F,
    ) -> Result<R, Self::Error>
    where
        F: FnOnce() -> R,
    {
        unsafe {
            // SAFETY: caller must ensure that `aspace` is valid and properly set up.
            let prev = mmu::switch_page_table(PhysAddr::new(self.rpt_pa));

            let ret = f();

            // SAFETY: we are restoring the kernel page table, which is always valid.
            mmu::switch_page_table(prev);

            Ok(ret)
        }
    }

    fn page_size(&self) -> u64 {
        PAGE_SIZE
    }

    fn rpt_pa(&self) -> u64 {
        self.rpt_pa
    }
}
