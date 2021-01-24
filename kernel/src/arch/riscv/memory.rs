use core::fmt;

use bitflags::bitflags;
use riscv::{PhysAddr, VirtAddr};

use crate::mm::phys::{bitmap::BitmapAllocator, AllocatorError, FrameAllocator, LockedAllocator};

use super::SATP;

const PAGE_SHIFT: u64 = 12;
const PAGE_SIZE: u64 = 1 << 12;

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

    fn get_pnn(&self) -> u64 {
        (self.inner.bits() >> PTE_PPN_OFFSET) & PTE_PPN_MASK
    }

    fn set_ppn(&mut self, ppn: u64) {
        self.inner.bits &= !(PTE_PPN_MASK << PTE_PPN_OFFSET);
        self.inner.bits |= (ppn & PTE_PPN_MASK) << PTE_PPN_OFFSET;
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
pub unsafe fn map(root: &mut PageTable, vaddr: VirtAddr, paddr: PhysAddr, flags: PteFields) {
    let vpn = [
        (u64::from(vaddr) >> 12) & 0x1ff,
        (u64::from(vaddr) >> 21) & 0x1ff,
        (u64::from(vaddr) >> 30) & 0x1ff,
    ];

    let mut table = root;

    for i in (0..=2).rev() {
        let pte = table.get_entry_mut(vpn[i] as usize).unwrap();

        if !pte.is_valid() {
            if i != 0 {
                // Allocate next level of page table
                let new_table_addr = u64::from(GFA.alloc_zeroed(1).unwrap());
                pte.set_flags(PteFields::VALID);
                pte.set_ppn(new_table_addr >> PAGE_SHIFT);
                table = (new_table_addr as *mut PageTable).as_mut().unwrap();
            } else {
                // Fill in leaf PTE
                pte.set_flags(flags | PteFields::VALID);
                pte.set_ppn(u64::from(paddr) >> PAGE_SHIFT);
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
pub unsafe fn virt_to_phys(root: &PageTable, vaddr: VirtAddr) -> Option<PhysAddr> {
    let vpn = [
        (u64::from(vaddr) >> 12) & 0x1ff,
        (u64::from(vaddr) >> 21) & 0x1ff,
        (u64::from(vaddr) >> 30) & 0x1ff,
    ];

    let mut table = root;

    for i in (0..=2).rev() {
        let pte = table.get_entry(vpn[i] as usize).unwrap();

        if !pte.is_valid() {
            break;
        } else if pte.is_leaf() {
            return Some(PhysAddr::new(pte.get_pnn() << PAGE_SHIFT) + vaddr.page_offset());
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
pub unsafe fn id_map_range(root: &mut PageTable, start: PhysAddr, end: PhysAddr, flags: PteFields) {
    let start = start.align_down(PAGE_SIZE);
    let end = end.align_up(PAGE_SIZE);

    let num_pages = u64::from(end - start) >> PAGE_SHIFT;

    for i in 0..num_pages {
        let addr = start + (i << PAGE_SHIFT);
        map(root, VirtAddr::new(addr.into()), addr, flags);
    }
}

/// Global frame allocator (GFA).
pub static mut GFA: LockedAllocator<BitmapAllocator<PAGE_SIZE>> = LockedAllocator::new();

/// Initializes a physical memory allocator on the specified memory range.
///
/// # Safety
///
/// There can be no guarantee that the memory being initialized isn't already in use by the system.
unsafe fn phys_init(mem_start: PhysAddr, mem_size: u64) -> Result<(), AllocatorError> {
    let mem_start = mem_start.align_up(PAGE_SIZE);
    let mem_end = (mem_start + mem_size).align_down(PAGE_SIZE);

    GFA.set_allocator(BitmapAllocator::<PAGE_SIZE>::init(mem_start, mem_end)?);

    Ok(())
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
        let kernel_mem_end = PhysAddr::new(&__end as *const usize as u64);

        let text_start = PhysAddr::new(&__text_start as *const usize as u64);
        let text_end = PhysAddr::new(&__text_end as *const usize as u64);
        let rodata_start = PhysAddr::new(&__rodata_start as *const usize as u64);
        let rodata_end = PhysAddr::new(&__rodata_end as *const usize as u64);
        let data_start = PhysAddr::new(&__data_start as *const usize as u64);
        let data_end = PhysAddr::new(&__data_end as *const usize as u64);

        kprintln!("Kernel memory map:");
        kprintln!("  [{} - {}] .text", text_start, text_end);
        kprintln!("  [{} - {}] .rodata", rodata_start, rodata_end);
        kprintln!("  [{} - {}] .data", data_start, data_end);

        // TODO: parse DTB to get the memory size
        let phys_mem_end = PhysAddr::new(0x8000_0000 + 128 * 1024 * 1024);

        // Configure physical memory
        phys_init(kernel_mem_end, (phys_mem_end - kernel_mem_end).into()).unwrap();

        // Setup root page table for virtual address translation
        let root = (u64::from(GFA.alloc_zeroed(1).unwrap()) as *mut PageTable)
            .as_mut()
            .unwrap();

        // Identity map all kernel sections
        id_map_range(
            root,
            PhysAddr::new(&__text_start as *const usize as u64),
            PhysAddr::new(&__text_end as *const usize as u64),
            PteFields::RX,
        );

        id_map_range(
            root,
            PhysAddr::new(&__rodata_start as *const usize as u64),
            PhysAddr::new(&__rodata_end as *const usize as u64),
            PteFields::RX,
        );

        id_map_range(
            root,
            PhysAddr::new(&__data_start as *const usize as u64),
            PhysAddr::new(&__data_end as *const usize as u64),
            PteFields::RW,
        );

        // Identity map UART0 memory
        id_map_range(
            root,
            PhysAddr::new(0x1000_0000),
            PhysAddr::new(0x1000_0100),
            PteFields::RW,
        );

        // Identity map CLINT memory
        id_map_range(
            root,
            PhysAddr::new(0x0200_0000),
            PhysAddr::new(0x0201_0000),
            PteFields::RW,
        );

        // Identity map SYSCON memory
        id_map_range(
            root,
            PhysAddr::new(0x0010_0000),
            PhysAddr::new(0x0010_1000),
            PteFields::RW,
        );

        // Enable MMU
        SATP.write((0x8 << 60) | ((root as *const _ as usize) >> PAGE_SHIFT));
    }
}
