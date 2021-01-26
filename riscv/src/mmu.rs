//! Abstractions for page tables and other paging related structures.

use core::fmt;

use bitflags::bitflags;
use kmm::{allocator::FrameAllocator, Align};

use crate::{PhysAddr, VirtAddr};

/// Number of bits that an address needs to be shifted to the left to obtain its page number.
pub const PAGE_SHIFT: u64 = 12;

/// Size of a page in bytes.
pub const PAGE_SIZE: u64 = 1 << 12;

const PTE_PPN_MASK: u64 = 0x3ff_ffff;
const PTE_PPN_OFFSET: u64 = 10;

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
    /// Returns a reference to an entry in this page table.
    pub fn get_entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// Returns a mutable reference to an entry in this page table.
    pub fn get_entry_mut(&mut self, i: usize) -> Option<&mut Entry> {
        self.entries.get_mut(i)
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

    /// Sets this entry's flags.
    pub fn set_flags(&mut self, flags: EntryFlags) {
        self.inner.insert(flags);
    }

    /// Returns the PPN portion of this entry.
    pub fn get_pnn(&self) -> u64 {
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
        write!(f, "phy: 0x{:016x} ", self.get_pnn() << PTE_PPN_OFFSET)?;
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

/// Maps the page containing the specified virtual address to a physical address, by creating
/// a new entry in the MMU page table with the provided flags.
///
/// Only 4 KiB pages are supported at the moment.
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
    root: &mut PageTable,
    vaddr: VirtAddr,
    paddr: PhysAddr,
    flags: EntryFlags,
    frame_allocator: &mut A,
) where
    A: FrameAllocator<PhysAddr, PAGE_SIZE>,
{
    let vpn = [
        (usize::from(vaddr) >> 12) & 0x1ff,
        (usize::from(vaddr) >> 21) & 0x1ff,
        (usize::from(vaddr) >> 30) & 0x1ff,
    ];

    let mut table = root;

    for i in (0..=2).rev() {
        let pte = table.get_entry_mut(vpn[i]).unwrap();

        if !pte.is_valid() {
            if i != 0 {
                // Allocate next level of page table
                let new_table_addr = u64::from(frame_allocator.alloc_zeroed(1).unwrap());
                pte.set_flags(EntryFlags::VALID);
                pte.set_ppn(new_table_addr >> PAGE_SHIFT);
                table = (new_table_addr as *mut PageTable).as_mut().unwrap();
            } else {
                // Fill in leaf PTE
                pte.set_flags(flags | EntryFlags::VALID);
                pte.set_ppn(u64::from(paddr) >> PAGE_SHIFT);
            }
        } else if i != 0 {
            // Descend to next table level
            table = ((pte.get_pnn() << PAGE_SHIFT) as *mut PageTable)
                .as_mut()
                .unwrap();
        } else if pte.flags() & EntryFlags::RWX != flags {
            panic!(
                "Entry {} already mapped with different flags ({:?} != {:?})",
                vaddr,
                pte.flags(),
                flags
            );
        }
    }
}

/// Returns the physical address mapped to the specified virtual address, by traversing the
/// provided page table.
#[allow(dead_code)]
pub fn virt_to_phys(root: &PageTable, vaddr: VirtAddr) -> Option<PhysAddr> {
    let vpn = [
        (usize::from(vaddr) >> 12) & 0x1ff,
        (usize::from(vaddr) >> 21) & 0x1ff,
        (usize::from(vaddr) >> 30) & 0x1ff,
    ];

    let mut table = root;

    for i in (0..=2).rev() {
        let pte = table.get_entry(vpn[i]).unwrap();

        if !pte.is_valid() {
            break;
        } else if pte.is_leaf() {
            return Some(PhysAddr::new(pte.get_pnn() << PAGE_SHIFT) + vaddr.page_offset() as u64);
        } else {
            table =
                unsafe { ((pte.get_pnn() << PAGE_SHIFT) as *const PageTable).as_ref() }.unwrap();
        }
    }

    None
}

/// Sets up identity mapping for a range of addresses, meaning that `vaddr == paddr` for all
/// addresses the specifed range.
///
/// # Safety
///
/// See [`map`] for safety consideration.
pub unsafe fn id_map_range<A>(
    root: &mut PageTable,
    start: PhysAddr,
    end: PhysAddr,
    flags: EntryFlags,
    frame_allocator: &mut A,
) where
    A: FrameAllocator<PhysAddr, PAGE_SIZE>,
{
    let start = start.align_down(PAGE_SIZE);
    let end = end.align_up(PAGE_SIZE);

    let num_pages = u64::from(end - start) >> PAGE_SHIFT;

    for i in 0..num_pages {
        let addr = start + (i << PAGE_SHIFT);
        map(
            root,
            VirtAddr::new(u64::from(addr) as usize),
            addr,
            flags,
            frame_allocator,
        );
    }
}
