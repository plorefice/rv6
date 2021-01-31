//! Abstractions for page tables and other paging related structures.

use core::{
    fmt,
    slice::{Iter, IterMut},
};

use bitflags::bitflags;
use kmm::{allocator::FrameAllocator, Align};

use crate::{
    addr::{PAGE_SHIFT, PAGE_SIZE},
    PhysAddr, VirtAddr,
};

#[cfg(feature = "sv39")]
const PTE_PPN_MASK: u64 = 0x3ff_ffff;
#[cfg(feature = "sv48")]
const PTE_PPN_MASK: u64 = 0xfff_ffff_ffff;

const PTE_PPN_OFFSET: u64 = 10;

#[cfg(feature = "sv39")]
const PAGE_LEVELS: usize = 3;
#[cfg(feature = "sv48")]
const PAGE_LEVELS: usize = 4;

bitflags! {
    /// Bitfields of a page table entry.
    pub struct EntryFlags: u64 {
        /// If set, this entry represents a valid mapping.
        const VALID = 1 << 0;
        /// If set, this page contains readable memory.
        const READ = 1 << 1;
        /// If set, this page contains writable memory.
        const WRITE = 1 << 2;
        /// If set, this page contains executable memory.
        const EXEC = 1 << 3;
        /// If set, this page can be accessed in U-mode.
        const USER = 1 << 4;
        /// If set, this mapping is global.
        const GLOBAL = 1 << 5;
        /// If set, this page has been accessed by the CPU.
        const ACCESS = 1 << 6;
        /// If set, this page has been written by the CPU.
        const DIRTY = 1 << 7;

        /// If set, this page contains read-write memory.
        const RW = Self::READ.bits | Self::WRITE.bits;
        /// If set, this page contains read-exec memory.
        const RX = Self::READ.bits | Self::EXEC.bits;
        /// If set, this page contains read-write-exec memory.
        const RWX = Self::READ.bits | Self::WRITE.bits | Self::EXEC.bits;
        /// Mask of user-settable flags on a page table entry.
        const RWXUG = Self::RWX.bits | Self::USER.bits | Self::GLOBAL.bits;

        /// Comination of all the above.
        const ALL = 0xf;
    }
}

/// A page table for virtual address translation.
#[derive(Debug)]
pub struct PageTable {
    entries: [Entry; 512],
}

impl PageTable {
    /// Resets all the entries of this page table to zero.
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }

    /// Returns a reference to an entry in this page table.
    pub fn get_entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// Returns a mutable reference to an entry in this page table.
    pub fn get_entry_mut(&mut self, i: usize) -> Option<&mut Entry> {
        self.entries.get_mut(i)
    }

    /// Returns an iterator over the entries in this page table.
    pub fn iter(&self) -> Iter<'_, Entry> {
        self.entries.iter()
    }

    /// Returns a mutable iterator over the entries in this page table.
    pub fn iter_mut(&mut self) -> IterMut<'_, Entry> {
        self.entries.iter_mut()
    }
}

impl fmt::Display for PageTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.entries.iter().enumerate() {
            if e.is_valid() {
                writeln!(f, "{:>3}: {}", i, e)?;
            }
        }
        Ok(())
    }
}

/// An entry in a `PageTable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entry {
    inner: EntryFlags,
}

impl Entry {
    /// Returns whether the mapping contained in this entry is valid for use in translation.
    pub fn is_valid(&self) -> bool {
        self.inner.contains(EntryFlags::VALID)
    }

    /// Returns whether the page pointed to by this entry is readable.
    pub fn is_read(&self) -> bool {
        self.inner.contains(EntryFlags::READ)
    }

    /// Returns whether the page pointed to by this entry is writable.
    pub fn is_write(&self) -> bool {
        self.inner.contains(EntryFlags::WRITE)
    }

    /// Returns whether the page pointed to by this entry contains executable code.
    pub fn is_exec(&self) -> bool {
        self.inner.contains(EntryFlags::EXEC)
    }

    /// Returns whether the page pointed to by this entry can be accessed in U-Mode.
    pub fn is_user(&self) -> bool {
        self.inner.contains(EntryFlags::USER)
    }

    /// Returns whether the mapping in this entry is global.
    ///
    /// Global mappings can be accessed in all address spaces.
    pub fn is_global(&self) -> bool {
        self.inner.contains(EntryFlags::GLOBAL)
    }

    /// Returns whether the virtual page that this mapping points to has been accessed
    /// since the last time this flag was cleared.
    pub fn is_accessed(&self) -> bool {
        self.inner.contains(EntryFlags::ACCESS)
    }

    /// Returns whether the virtual page that this mapping points to has been written
    /// since the last time this flag was cleared.
    pub fn is_dirty(&self) -> bool {
        self.inner.contains(EntryFlags::DIRTY)
    }

    /// Returns whether this entry is a leaf or a pointer to another page table.
    ///
    /// Equivalent to `self.is_read() || self.is_write() || self.is_exec()`.
    pub fn is_leaf(&self) -> bool {
        self.inner
            .intersects(EntryFlags::READ | EntryFlags::WRITE | EntryFlags::EXEC)
    }

    /// Returns the flags currently set on this entry.
    pub fn flags(&self) -> EntryFlags {
        self.inner & EntryFlags::ALL
    }

    /// Resets the bits of this entry to zero.
    pub fn clear(&mut self) {
        self.inner.bits = 0;
    }

    /// Sets this entry's flags.
    pub fn set_flags(&mut self, flags: EntryFlags) {
        self.inner &= !EntryFlags::ALL;
        self.inner |= flags;
    }

    /// Returns the PPN portion of this entry.
    pub fn get_ppn(&self) -> u64 {
        (self.inner.bits() >> PTE_PPN_OFFSET) & PTE_PPN_MASK
    }

    /// Sets the PPN portion of this entry to the provided value.
    pub fn set_ppn(&mut self, ppn: u64) {
        self.inner.bits &= !(PTE_PPN_MASK << PTE_PPN_OFFSET);
        self.inner.bits |= (ppn & PTE_PPN_MASK) << PTE_PPN_OFFSET;
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phy: 0x{:016x} ", self.get_ppn() << PTE_PPN_OFFSET)?;
        write!(
            f,
            "{} {} {} {} {} {} {}",
            if self.is_read() { 'R' } else { ' ' },
            if self.is_write() { 'W' } else { ' ' },
            if self.is_exec() { 'X' } else { ' ' },
            if self.is_user() { 'U' } else { ' ' },
            if self.is_global() { 'G' } else { ' ' },
            if self.is_accessed() { 'A' } else { ' ' },
            if self.is_dirty() { 'D' } else { ' ' },
        )
    }
}

/// Possible sizes for page table mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PageSize {
    /// 4KiB page.
    Kb,
    /// 2MiB _megapage_.
    Mb,
    /// 1GiB _gigapage_.
    Gb,
    /// 512GiB _terapage_.
    #[cfg(feature = "sv48")]
    Tb,
}

impl PageSize {
    /// Converts this page size to the page table level which this page maps to.
    fn to_table_level(self) -> usize {
        match self {
            PageSize::Kb => 0,
            PageSize::Mb => 1,
            PageSize::Gb => 2,
            #[cfg(feature = "sv48")]
            PageSize::Tb => 3,
        }
    }
}

/// An error condition returned by memory mapping functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapError {
    /// The requested page was already mapped with different flags.
    AlreadyMapped,
    /// Frame allocation failed.
    AllocationFailed,
}

/// A memory mapper that requires that the whole physical address space is mapped at some offset
/// in the virtual address space.
#[derive(Debug)]
pub struct OffsetPageMapper<'a> {
    rpt: &'a mut PageTable,
    phys_offset: VirtAddr,
}

impl<'a> OffsetPageMapper<'a> {
    /// Creates a new mapper that uses the given offset to translate physical to virtual addresses.
    ///
    /// The complete physical memory must be mapped in the virtual address space starting at
    /// `phys_offset`. This is required because the mapper must access page tables, which are not
    /// mapped to virtual memory by default.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `rpt` points to a valid root page table, and all physical
    /// memory has been mapped at `phys_offset`.
    pub unsafe fn new(rpt: &'a mut PageTable, phys_offset: VirtAddr) -> Self {
        Self { rpt, phys_offset }
    }

    /// Maps a memory page of size `page_size` using the provided root page table.
    ///
    /// The created mapping will translate addresses in the page `vaddr` to physical addresses in the
    /// frame `paddr` according to the specified flags.
    ///
    /// If necessary, new frames for page tables will be allocated using `frame_allocator`.
    ///
    /// # Safety
    ///
    /// This operation is fundamentally unsafe because it provides several opportunities to break
    /// memory safety guarantees. For example, re-mapping a page to a different frame will invalidate
    /// all the references within that page.
    ///
    /// It is up to the caller to guarantee that no undefined behavior or memory violations can occur
    /// through the new mapping.
    pub unsafe fn map<A>(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        page_size: PageSize,
        mut flags: EntryFlags,
        frame_allocator: &mut A,
    ) -> Result<(), MapError>
    where
        A: FrameAllocator<PhysAddr, PAGE_SIZE>,
    {
        #[cfg(feature = "sv39")]
        let vpn = [vaddr.vpn0(), vaddr.vpn1(), vaddr.vpn2()];

        #[cfg(feature = "sv48")]
        let vpn = [vaddr.vpn0(), vaddr.vpn1(), vaddr.vpn2(), vaddr.vpn3()];

        let mut pte = self.rpt.get_entry_mut(vpn[PAGE_LEVELS - 1]).unwrap();

        for i in (page_size.to_table_level()..PAGE_LEVELS - 1).rev() {
            // Traverse page table entry to the next level, or allocate a new level of page table
            let table_paddr = if !pte.is_valid() {
                let new_table_addr =
                    unsafe { frame_allocator.alloc(1) }.ok_or(MapError::AllocationFailed)?;
                pte.clear();
                pte.set_flags(EntryFlags::VALID);
                pte.set_ppn(new_table_addr.page_index());
                new_table_addr
            } else {
                PhysAddr::new(pte.get_ppn() << PAGE_SHIFT)
            };

            let table = unsafe {
                self.phys_to_virt(table_paddr)
                    .as_mut_ptr::<PageTable>()
                    .as_mut()
                    .unwrap()
            };

            pte = table.get_entry_mut(vpn[i]).unwrap();
        }

        // Normalize flags
        flags &= EntryFlags::RWXUG;

        // Check if a mapping already exists with different flags.
        if pte.is_valid() && pte.flags() & EntryFlags::RWXUG != flags {
            return Err(MapError::AlreadyMapped);
        }

        // Fill in leaf PTE
        pte.set_flags(flags | EntryFlags::VALID);
        pte.set_ppn(paddr.page_index());

        Ok(())
    }

    /// Sets up identity mapping for a range of addresses, meaning that `vaddr == paddr` for all
    /// addresses the specifed range.
    ///
    /// # Safety
    ///
    /// See [`map`] for safety consideration.
    pub unsafe fn identity_map_range<A>(
        &mut self,
        start: PhysAddr,
        end: PhysAddr,
        flags: EntryFlags,
        frame_allocator: &mut A,
    ) -> Result<(), MapError>
    where
        A: FrameAllocator<PhysAddr, PAGE_SIZE>,
    {
        let start = start.align_down(PAGE_SIZE);
        let end = end.align_up(PAGE_SIZE);

        let num_pages = u64::from(end - start) >> PAGE_SHIFT;

        for i in 0..num_pages {
            let addr = start + (i << PAGE_SHIFT);

            unsafe {
                self.map(
                    VirtAddr::new(u64::from(addr) as usize),
                    addr,
                    PageSize::Kb,
                    flags,
                    frame_allocator,
                )?;
            }
        }

        Ok(())
    }

    /// Translates a physical address into the corresponding virtual address.
    ///
    /// Since this mapper assumes that all physical memory is mapped to the virtual address space,
    /// this operation is trivial and will always succeed.
    pub fn phys_to_virt(&self, paddr: PhysAddr) -> VirtAddr {
        VirtAddr::new(paddr.data() as usize) + self.phys_offset
    }

    /// Returns the physical address mapped to the specified virtual address, or `None` if the
    /// address is not mapped.
    pub fn virt_to_phys(&self, vaddr: VirtAddr) -> Option<PhysAddr> {
        #[cfg(feature = "sv39")]
        let vpn = [
            (vaddr.data() >> 12) & 0x1ff,
            (vaddr.data() >> 21) & 0x1ff,
            (vaddr.data() >> 30) & 0x1ff,
        ];

        #[cfg(feature = "sv48")]
        let vpn = [
            (vaddr.data() >> 12) & 0x1ff,
            (vaddr.data() >> 21) & 0x1ff,
            (vaddr.data() >> 30) & 0x1ff,
            (vaddr.data() >> 39) & 0x1ff,
        ];

        let mut table = &*self.rpt;

        for i in (0..PAGE_LEVELS).rev() {
            let pte = table.get_entry(vpn[i]).unwrap();

            if !pte.is_valid() {
                break;
            }

            if pte.is_leaf() {
                let mut ppn = pte.get_ppn();

                // For i > 0, the lower bits of PPN are taken from the virtual address
                for (lvl, vpn) in vpn.iter().enumerate().take(i) {
                    ppn |= (vpn << (lvl * 9)) as u64;
                }

                return Some(PhysAddr::new(ppn << PAGE_SHIFT) + vaddr.page_offset() as u64);
            }

            table = unsafe {
                self.phys_to_virt(PhysAddr::from_ppn(pte.get_ppn()))
                    .as_ptr::<PageTable>()
                    .as_ref()
                    .unwrap()
            };
        }

        None
    }
}
