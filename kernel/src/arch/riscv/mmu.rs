//! Abstractions for page tables and other paging related structures.

use core::{
    fmt,
    ops::Range,
    slice::{Iter, IterMut},
};

use bitflags::bitflags;

use crate::{
    arch::{
        pa_to_va,
        riscv::addr::{PhysAddr, VirtAddr, PAGE_SHIFT, PAGE_SIZE},
    },
    mm::{allocator::FrameAllocator, Align},
};

#[cfg(all(feature = "sv39", feature = "sv48"))]
compile_error!("Features \"sv39\" and \"sv48\" are mutually exclusive.");

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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[repr(transparent)]
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
        /// Reserved for use by supervisor software
        const RSW = 3 << 8;
        /// Page-based memory types
        const PBMT = 3 << 61;
        /// Naturally aligned power-of-2 table entry
        const NAPOT = 1 << 63;

        /// If set, the pages is shareable (C9xx specific)
        const SHARE = 1 << 60;
        /// If set, the pages is bufferable (C9xx specific)
        const BUF = 1 << 61;
        /// If set, the pages is cacheable (C9xx specific)
        const CACHE = 1 << 62;

        /// If set, this page contains read-write memory.
        const RW = Self::READ.bits() | Self::WRITE.bits();
        /// If set, this page contains read-exec memory.
        const RX = Self::READ.bits() | Self::EXEC.bits();
        /// If set, this page contains read-write-exec memory.
        const RWX = Self::READ.bits() | Self::WRITE.bits() | Self::EXEC.bits();
        /// Mask of user-settable flags on a page table entry.
        const RWXUG = Self::RWX.bits() | Self::USER.bits() | Self::GLOBAL.bits();

        /// PTE flags for kernel mappings
        const KERNEL = Self::RWX.bits() | Self::ACCESS.bits() | Self::DIRTY.bits() | Self::GLOBAL.bits();
        /// PTE flags for MMIO mappings
        const MMIO = Self::RW.bits() | Self::ACCESS.bits() | Self::DIRTY.bits() | Self::GLOBAL.bits();

        /// PTE flags for user mappings
        const USER_RX = Self::RX.bits() | Self::USER.bits() | Self::ACCESS.bits();
        const USER_RW = Self::RW.bits() | Self::USER.bits() | Self::ACCESS.bits() | Self::DIRTY.bits();
    }
}

/// A page table for virtual address translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(align(4096))]
pub struct PageTable {
    entries: [Entry; 512],
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PageTable {
    /// Creates a new page table with cleared entries.
    pub const fn new() -> Self {
        Self {
            entries: [Entry::empty(); 512],
        }
    }

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
#[repr(transparent)]
pub struct Entry {
    inner: EntryFlags,
}

impl Entry {
    /// Create a new empty, non-valid page entry.
    pub const fn empty() -> Entry {
        Self {
            inner: EntryFlags::empty(),
        }
    }

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
        // Mask away the PPN bits
        self.inner & EntryFlags::all()
    }

    /// Resets the bits of this entry to zero.
    pub fn clear(&mut self) {
        self.inner = EntryFlags::empty();
    }

    /// Sets this entry's flags.
    pub fn set_flags(&mut self, flags: EntryFlags) {
        self.inner |= flags;
    }

    /// Returns the PPN portion of this entry.
    pub fn get_ppn(&self) -> u64 {
        (self.inner.bits() >> PTE_PPN_OFFSET) & PTE_PPN_MASK
    }

    /// Sets the PPN portion of this entry to the provided value.
    pub fn set_ppn(&mut self, ppn: u64) {
        let mut v = self.inner.bits();
        v &= !(PTE_PPN_MASK << PTE_PPN_OFFSET);
        v |= (ppn & PTE_PPN_MASK) << PTE_PPN_OFFSET;
        self.inner = EntryFlags::from_bits_retain(v);
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phy: 0x{:016x} ", self.get_ppn() << PAGE_SHIFT)?;
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

    fn from_table_level(lvl: usize) -> Option<Self> {
        match lvl {
            0 => Some(PageSize::Kb),
            1 => Some(PageSize::Mb),
            2 => Some(PageSize::Gb),
            #[cfg(feature = "sv48")]
            3 => Some(PageSize::Tb),
            _ => None,
        }
    }

    /// Returns the number of bytes contained in a page of this size.
    pub const fn size(self) -> u64 {
        match self {
            PageSize::Kb => 0x1000,
            PageSize::Mb => 0x200000,
            PageSize::Gb => 0x4000_0000,
            #[cfg(feature = "sv48")]
            PageSize::Tb => 0x80_0000_0000,
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

/// A simple memory mapper.
#[derive(Debug)]
pub struct PageTableWalker<'a> {
    rpt: &'a mut PageTable,
}

impl<'a> PageTableWalker<'a> {
    /// Creates a new page mapper.
    ///
    /// # Safety
    ///
    /// It is assumed that a physical-to-virtual address translation is possible for all physical
    /// memory addresses using the [`pa_to_va`] function. This is required because the mapper must
    /// access page tables, which are not mapped to virtual memory by default.
    ///
    /// The caller must also guarantee that `rpt` points to a valid root page table.
    pub unsafe fn new(rpt: &'a mut PageTable) -> Self {
        Self { rpt }
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
    pub unsafe fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        page_size: PageSize,
        mut flags: EntryFlags,
        allocator: &mut impl FrameAllocator<PhysAddr, PAGE_SIZE>,
    ) -> Result<(), MapError> {
        #[cfg(feature = "sv39")]
        let vpn = [vaddr.vpn0(), vaddr.vpn1(), vaddr.vpn2()];

        #[cfg(feature = "sv48")]
        let vpn = [vaddr.vpn0(), vaddr.vpn1(), vaddr.vpn2(), vaddr.vpn3()];

        let mut pte = self.rpt.get_entry_mut(vpn[PAGE_LEVELS - 1]).unwrap();

        for i in (page_size.to_table_level()..PAGE_LEVELS - 1).rev() {
            // Traverse page table entry to the next level, or allocate a new level of page table
            let table_paddr = if !pte.is_valid() {
                // SAFETY: PageTable fits in a single 4k page
                let frame = unsafe { allocator.alloc(1).ok_or(MapError::AllocationFailed)? };
                let new_table_addr = frame.paddr;

                pte.clear();
                pte.set_flags(EntryFlags::VALID);
                pte.set_ppn(new_table_addr.page_index());

                // Initialize the newly allocated page table
                // SAFETY: new_table_addr points to valid writable memory
                unsafe {
                    (frame.ptr as *mut PageTable).write(PageTable::default());
                }

                new_table_addr
            } else {
                PhysAddr::new(pte.get_ppn() << PAGE_SHIFT)
            };

            // SAFETY: the resulting pointer points to properly initialized memory
            let table = unsafe { &mut *pa_to_va(table_paddr).as_mut_ptr::<PageTable>() };

            pte = table.get_entry_mut(vpn[i]).unwrap();
        }

        // Activate mapping
        flags |= EntryFlags::VALID;

        // Check if a mapping already exists with different flags
        if pte.is_valid() && pte.flags() != flags {
            return Err(MapError::AlreadyMapped);
        }

        // Fill in leaf PTE
        pte.set_flags(flags);
        pte.set_ppn(paddr.page_index());

        Ok(())
    }

    /// Maps a range of addresses to pages of size `page_size` starting at `vaddr`.
    /// See also [`Self::map`].
    ///
    /// # Safety
    ///
    /// See [`Self::map`] for safety consideration.
    pub unsafe fn map_range(
        &mut self,
        vaddr: VirtAddr,
        phys: Range<PhysAddr>,
        page_size: PageSize,
        flags: EntryFlags,
        allocator: &mut impl FrameAllocator<PhysAddr, PAGE_SIZE>,
    ) -> Result<(), MapError> {
        let start = phys.start;
        let end = phys.end;

        let sz = (end - start).data();
        let n_pages = sz.div_ceil(page_size.size());

        for i in 0..n_pages {
            let offset = i * page_size.size();

            // SAFETY: assuming caller has upheld the safety contract
            unsafe {
                self.map(
                    vaddr + offset as usize,
                    start + offset,
                    page_size,
                    flags,
                    allocator,
                )?;
            }
        }

        Ok(())
    }

    /// Sets up identity mapping for a range of addresses, meaning that `vaddr == paddr` for all
    /// addresses the specifed range.
    ///
    /// # Safety
    ///
    /// See [`map`] for safety consideration.
    pub unsafe fn identity_map_range(
        &mut self,
        start: PhysAddr,
        end: PhysAddr,
        flags: EntryFlags,
        allocator: &mut impl FrameAllocator<PhysAddr, PAGE_SIZE>,
    ) -> Result<(), MapError> {
        let start = start.align_down(PAGE_SIZE);
        let end = end.align_up(PAGE_SIZE);

        let num_pages = u64::from(end - start) >> PAGE_SHIFT;

        for i in 0..num_pages {
            let addr = start + (i << PAGE_SHIFT);

            // SAFETY: assuming caller has upheld the safety contract
            unsafe {
                self.map(
                    VirtAddr::new(u64::from(addr) as usize),
                    addr,
                    PageSize::Kb,
                    flags,
                    allocator,
                )?;
            }
        }

        Ok(())
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

            // SAFETY: if this PTE is valid then the PPN points to valid memory
            table = unsafe { &*pa_to_va(PhysAddr::from_ppn(pte.get_ppn())).as_ptr::<PageTable>() };
        }

        None
    }

    /// Returns a reference to the root page table used by this mapper.
    pub fn page_table(&self) -> &PageTable {
        self.rpt
    }
}

/// Dumps the current page mappings to the kernel console.
///
/// Useful to debug the state of virtual memory.
///
/// # Safety
///
/// It is assumed that `pt` points to the root page table. If not, this function might perform
/// invalid memory accesses.
pub unsafe fn dump_root_page_table(pt: &PageTable) {
    kprintln!("Active memory mappings:");
    kprintln!("  vaddr            paddr            size             attr   ");
    kprintln!("  ---------------- ---------------- ---------------- -------");
    if let Some(mapping) = _dump_page_table(pt, VirtAddr::new_truncated(0), PAGE_LEVELS - 1, None) {
        kprintln!("{mapping}");
    }
}

/// Recursively gathers active mappings and dump a coalesced view of the mapped memory.
///
/// Returns the last mapped chunk, if any. It's up to the caller to print it as well.
fn _dump_page_table(
    pt: &PageTable,
    base: VirtAddr,
    level: usize,
    mut mapping: Option<MemoryMappingInfo>,
) -> Option<MemoryMappingInfo> {
    let base = VirtAddr::new(base.data() << 9);

    for (i, entry) in pt.entries.iter().enumerate().filter(|(_, e)| e.is_valid()) {
        let vaddr = base + i;

        if entry.is_leaf() {
            let virt = VirtAddr::new(vaddr.data() << (9 * level) << PAGE_SHIFT);
            let phys = PhysAddr::from_ppn(entry.get_ppn());
            let size = PageSize::from_table_level(level).unwrap().size();
            let flags = entry.flags();

            // Check if this mapping can be merged with the current chunk...
            if let Some(ref mut mapping) = mapping {
                if virt == mapping.virt + mapping.size as usize
                    && phys == mapping.phys + mapping.size
                    && mapping.flags == flags
                {
                    mapping.size += size;
                    continue; // ... if so, accrue its size and keep iterating over this table ...
                }
                // ... if not, print the previous chunk and restart a new chunk from here
                kprintln!("{mapping}");
            }

            // First mapping or disjoined chunk found: start building a new one
            mapping = Some(MemoryMappingInfo {
                phys,
                virt,
                size,
                flags,
            });
        } else {
            // SAFETY: we are traversing a page table, so we assume that the corresponding virtual
            //         memory has been properly mapped
            let inner = unsafe { pa_to_va(PhysAddr::from_ppn(entry.get_ppn())) };

            // SAFETY: non-leaf PTEs point to other page tables
            let inner = unsafe { &*inner.as_mut_ptr::<PageTable>() };

            assert!(level > 0);
            mapping = _dump_page_table(inner, vaddr, level - 1, mapping);
        }
    }

    mapping
}

#[derive(Debug, Clone, Copy)]
struct MemoryMappingInfo {
    virt: VirtAddr,
    phys: PhysAddr,
    size: u64,
    flags: EntryFlags,
}

impl fmt::Display for MemoryMappingInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[rustfmt::skip]
        let r = write!(
            f,
            "  {:016} {:016} {:016x} {}{}{}{}{}{}{}",
            self.virt,
            self.phys,
            self.size,
            if self.flags.contains(EntryFlags::READ)   { 'r' } else { '-' },
            if self.flags.contains(EntryFlags::WRITE)  { 'w' } else { '-' },
            if self.flags.contains(EntryFlags::EXEC)   { 'x' } else { '-' },
            if self.flags.contains(EntryFlags::USER)   { 'u' } else { '-' },
            if self.flags.contains(EntryFlags::GLOBAL) { 'g' } else { '-' },
            if self.flags.contains(EntryFlags::ACCESS) { 'a' } else { '-' },
            if self.flags.contains(EntryFlags::DIRTY)  { 'd' } else { '-' },
        );

        r
    }
}
