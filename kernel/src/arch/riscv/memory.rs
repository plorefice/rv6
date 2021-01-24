use core::fmt;

use bitflags::bitflags;

use crate::mm::{
    self,
    page::{Address, PAGE_SHIFT, PAGE_SIZE},
    phys::{FrameAllocator, PhysicalAddress, GFA},
    virt::VirtualAddress,
};

use super::SATP;

const PTE_PPN_MASK: u64 = 0x3ff_ffff;
const PTE_PPN_OFFSET: u64 = 10;

bitflags! {
    /// Bitfields of a page table entry.
    pub struct PteFields: u64 {
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
pub struct PageTable {
    entries: [Entry; 512],
}

impl PageTable {
    fn get_entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    fn get_entry_mut(&mut self, i: usize) -> Option<&mut Entry> {
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

/// A page table entry.
pub struct Entry {
    inner: PteFields,
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

impl Entry {
    fn is_valid(&self) -> bool {
        self.inner.contains(PteFields::VALID)
    }

    fn is_read(&self) -> bool {
        self.inner.contains(PteFields::READ)
    }

    fn is_write(&self) -> bool {
        self.inner.contains(PteFields::WRITE)
    }

    fn is_exec(&self) -> bool {
        self.inner.contains(PteFields::EXEC)
    }

    fn is_user(&self) -> bool {
        self.inner.contains(PteFields::USER)
    }

    fn is_global(&self) -> bool {
        self.inner.contains(PteFields::GLOBAL)
    }

    fn is_accessed(&self) -> bool {
        self.inner.contains(PteFields::ACCESS)
    }

    fn is_dirty(&self) -> bool {
        self.inner.contains(PteFields::DIRTY)
    }

    fn is_leaf(&self) -> bool {
        self.inner
            .intersects(PteFields::READ | PteFields::WRITE | PteFields::EXEC)
    }

    fn flags(&self) -> PteFields {
        self.inner & PteFields::ALL
    }

    fn set_flags(&mut self, flags: PteFields) {
        self.inner.insert(flags);
    }

    fn get_pnn(&self) -> usize {
        ((self.inner.bits() >> PTE_PPN_OFFSET) & PTE_PPN_MASK) as usize
    }

    fn set_ppn(&mut self, ppn: usize) {
        self.inner.bits &= !(PTE_PPN_MASK << PTE_PPN_OFFSET);
        self.inner.bits |= (ppn as u64 & PTE_PPN_MASK) << PTE_PPN_OFFSET;
    }
}

/// Maps the page containing the specified virtual address to a physical address, by creating
/// a new entry in the MMU page table with the provided flags.
///
/// Only 4 KiB pages are supported at the moment.
///
/// # Safety
///
/// All low-level memory operations are intrinsically unsafe.
pub unsafe fn map(
    root: &mut PageTable,
    vaddr: VirtualAddress,
    paddr: PhysicalAddress,
    flags: PteFields,
) {
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
                let new_table_addr = usize::from(GFA.alloc_zeroed(1).unwrap());
                pte.set_flags(PteFields::VALID);
                pte.set_ppn(new_table_addr >> PAGE_SHIFT);
                table = (new_table_addr as *mut PageTable).as_mut().unwrap();
            } else {
                // Fill in leaf PTE
                pte.set_flags(flags | PteFields::VALID);
                pte.set_ppn(usize::from(paddr) >> PAGE_SHIFT);
            }
        } else if i != 0 {
            // Descend to next table level
            table = ((pte.get_pnn() << PAGE_SHIFT) as *mut PageTable)
                .as_mut()
                .unwrap();
        } else if pte.flags() & PteFields::RWX != flags {
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
pub unsafe fn virt_to_phys(root: &PageTable, vaddr: VirtualAddress) -> Option<PhysicalAddress> {
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
            return Some(PhysicalAddress::new(pte.get_pnn() << PAGE_SHIFT) + vaddr.page_offset());
        } else {
            table = ((pte.get_pnn() << PAGE_SHIFT) as *const PageTable)
                .as_ref()
                .unwrap();
        }
    }

    None
}

/// Sets up identity mapping for a range of addresses, meaning that `vaddr == paddr` for all
/// addresses the specifed range.
pub unsafe fn id_map_range(
    root: &mut PageTable,
    start: PhysicalAddress,
    end: PhysicalAddress,
    flags: PteFields,
) {
    let start = start.align_to_previous_page(PAGE_SIZE);
    let end = end.align_to_next_page(PAGE_SIZE);

    let num_pages = usize::from(end - start) >> PAGE_SHIFT;

    for i in 0..num_pages {
        let addr = start + (i << PAGE_SHIFT);
        map(root, VirtualAddress::new(addr.into()), addr, flags);
    }
}

/// Initializes the system memory, by setting up a frame allocator and enabling virtual memory.
pub fn init() {
    // Defined in linker script
    extern "C" {
        /// The starting word of the kernel in memory.
        static __start: usize;
        /// The ending word of the kernel in memory.
        static __end: usize;
        /// The starting word of the text section in memory.
        static __text_start: usize;
        /// The ending word of the text section in memory.
        static __text_end: usize;
        /// The starting word of the RO data section in memory.
        static __rodata_start: usize;
        /// The ending word of the RO data section in memory.
        static __rodata_end: usize;
        /// The starting word of the data section in memory.
        static __data_start: usize;
        /// The ending word of the data section in memory.
        static __data_end: usize;
    }

    unsafe {
        let kernel_mem_end = PhysicalAddress::new(&__end as *const usize as usize);

        let text_start = PhysicalAddress::new(&__text_start as *const usize as usize);
        let text_end = PhysicalAddress::new(&__text_end as *const usize as usize);
        let rodata_start = PhysicalAddress::new(&__rodata_start as *const usize as usize);
        let rodata_end = PhysicalAddress::new(&__rodata_end as *const usize as usize);
        let data_start = PhysicalAddress::new(&__data_start as *const usize as usize);
        let data_end = PhysicalAddress::new(&__data_end as *const usize as usize);

        kprintln!("Kernel memory map:");
        kprintln!("  [{} - {}] .text", text_start, text_end);
        kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
        kprintln!("  [{} - {}] .data", data_start, data_end);

        // TODO: parse DTB to get the memory size
        let phys_mem_end = PhysicalAddress::new(0x8000_0000 + 128 * 1024 * 1024);

        // Configure physical memory
        mm::phys::init(kernel_mem_end, (phys_mem_end - kernel_mem_end).into()).unwrap();

        // Setup root page table for virtual address translation
        let root = (usize::from(GFA.alloc_zeroed(1).unwrap()) as *mut PageTable)
            .as_mut()
            .unwrap();

        // Identity map all kernel sections
        id_map_range(
            root,
            PhysicalAddress::new(&__text_start as *const usize as usize),
            PhysicalAddress::new(&__text_end as *const usize as usize),
            PteFields::RX,
        );

        id_map_range(
            root,
            PhysicalAddress::new(&__rodata_start as *const usize as usize),
            PhysicalAddress::new(&__rodata_end as *const usize as usize),
            PteFields::RX,
        );

        id_map_range(
            root,
            PhysicalAddress::new(&__data_start as *const usize as usize),
            PhysicalAddress::new(&__data_end as *const usize as usize),
            PteFields::RW,
        );

        // Identity map UART0 memory
        id_map_range(root, 0x1000_0000.into(), 0x1000_0100.into(), PteFields::RW);

        // Identity map CLINT memory
        id_map_range(root, 0x0200_0000.into(), 0x0201_0000.into(), PteFields::RW);

        // Identity map SYSCON memory
        id_map_range(root, 0x0010_0000.into(), 0x0010_1000.into(), PteFields::RW);

        // Enable MMU
        SATP.write((0x8 << 60) | ((root as *const _ as usize) >> PAGE_SHIFT));
    }
}
