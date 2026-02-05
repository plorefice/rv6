//! Process loading and initialization.

use core::fmt;

use elf::Elf64;

use crate::mm::addr::{Align, MemoryAddress, VirtAddr};

/// Trait defining the architecture-specific interface for loading processes.
/// The core process loader will call these methods to set up the process's address space and load
/// the ELF segments. This allows the core loader logic to be mostly architecture-agnostic,
/// while still giving the architecture control over how memory is allocated and mapped.
pub trait ElfLoader {
    /// An opaque handle representing the process's address space, which the core loader will pass to
    /// the arch loader for memory operations.
    type AddrSpace;

    /// An opaque error type for reporting architecture-specific loading failures.
    type Error: fmt::Debug;

    /// Creates a new user address space and return its handle.
    fn new_user_addr_space(&self) -> Result<Self::AddrSpace, Self::Error>;

    /// Choose a base for PIE within your user VA layout constraints.
    /// Core can call this when it detects ET_DYN.
    fn choose_pie_base(
        &self,
        aspace: &mut Self::AddrSpace,
        image_min_vaddr: VirtAddr,
        image_max_vaddr: VirtAddr,
        align: usize,
        hint: usize,
    ) -> Result<usize, Self::Error>;

    /// Validates that a virtual address range is permitted for user mappings.
    fn validate_user_range(
        &self,
        aspace: &Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
    ) -> Result<(), Self::Error>;

    /// Maps anonymous pages for [vaddr, vaddr+len) with flags.
    /// Core will do page rounding; arch can assume page-aligned.
    fn map_anonymous(
        &self,
        aspace: &mut Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
        flags: SegmentFlags,
    ) -> Result<(), Self::Error>;

    /// Updates the permissions of an already-mapped range.
    fn protect_range(
        &self,
        aspace: &mut Self::AddrSpace,
        vaddr: VirtAddr,
        len: usize,
        flags: SegmentFlags,
    ) -> Result<(), Self::Error>;

    /// Copies bytes into an already-mapped region.
    fn copy_to_user(
        &self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: VirtAddr,
        src: &[u8],
    ) -> Result<(), Self::Error>;

    /// Zero fills an already-mapped region.
    fn zero_user(
        &self,
        aspace: &mut Self::AddrSpace,
        dst_vaddr: VirtAddr,
        len: usize,
    ) -> Result<(), Self::Error>;

    /// Optional but common: after mapping code, ensures instruction cache coherency.
    fn finalize_image(
        &self,
        aspace: &mut Self::AddrSpace,
        mapped_exec_ranges: &[(VirtAddr, VirtAddr)],
    ) -> Result<(), Self::Error>;

    /// Provides page size / alignment constraints (core uses to round).
    fn page_size(&self) -> usize;
}

/// Instruction for the core loader on how to load an ELF image.
///
/// The core loader will fill in the `segments` array based on the ELF program headers, and then
/// call the arch loader to allocate/mmap memory and copy the segments as needed.
#[derive(Debug, Clone, Copy)]
pub struct LoadPlan<'a> {
    /// Entry point VA (including PIE base if applicable)
    pub entry: VirtAddr,
    /// caller-provided buffer filled by core
    pub segments: &'a [LoadSegment<'a>],
}

/// A single segment to be loaded, derived from an ELF PT_LOAD program header.
#[derive(Default, Debug, Clone, Copy)]
pub struct LoadSegment<'a> {
    /// Final VA (already includes base for PIE if applied by core)
    pub vaddr: VirtAddr,
    /// Size in memory
    pub mem_size: usize,
    /// Data to copy from the file
    pub file_data: &'a [u8],
    /// Offset in the file
    pub file_off: usize,
    /// Segment flags
    pub flags: SegmentFlags,
    /// Alignment constraint
    pub align: usize,
}

bitflags::bitflags! {
    /// Flags for a loadable segment.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SegmentFlags: u32 {
        /// Readable segment
        const R = 0b001;
        /// Writable segment
        const W = 0b010;
        /// Executable segment
        const X = 0b100;
    }
}

/// Policy for loading an ELF image, which the core loader can use to enforce certain constraints on
/// the loading process.
#[derive(Debug, Clone, Copy)]
pub struct LoadPolicy {
    /// Forbid W+X if false
    pub allow_wx: bool,
    /// Optional base hint (ASLR can ignore)
    pub pie_base_hint: usize,
    /// Cap PT_LOAD count
    pub max_segments: usize,
}
fn build_load_plan<'a>(
    elf: &'a [u8],
    policy: LoadPolicy,
    ph_buf: &'a mut [LoadSegment<'a>],
) -> Result<LoadPlan<'a>, ElfLoadError> {
    // parse header, program headers
    // filter PT_LOAD, validate overlap, compute image bounds
    // fill ph_buf[..n] with LoadSegment { vaddr: p_vaddr (+base later), file_data: ..., flags: ... }
    // decide base=0 for ET_EXEC; base=policy.pie_base_hint placeholder for ET_DYN (resolved in apply)
    // return LoadPlan { segments: &ph_buf[..n], entry: e_entry (+base if PIE), ... }

    let elf = Elf64::parse(elf)?;

    for (i, ph) in elf.program_headers().enumerate() {
        let ph = ph?;

        // We only care about loadable segments
        if !ph.is_load() {
            continue;
        }

        // Check available space in ph_buf
        if i >= ph_buf.len() || i >= policy.max_segments {
            return Err(ElfLoadError::TooManySegments);
        }

        let file_data = elf.segment_data(&ph)?;
        let mem_size = ph.memsz() as usize;
        let vaddr = VirtAddr::new(ph.vaddr() as usize);
        let file_off = ph.offset() as usize;
        let align = ph.align() as usize;

        let flags = {
            let mut f = SegmentFlags::empty();
            if ph.is_readable() {
                f |= SegmentFlags::R;
            }
            if ph.is_writable() {
                f |= SegmentFlags::W;
            }
            if ph.is_executable() {
                f |= SegmentFlags::X;
            }
            f
        };

        // Validate segment
        if align == 0 || !align.is_power_of_two() {
            return Err(ElfLoadError::Misaligned);
        }
        if !vaddr.is_aligned(align) {
            return Err(ElfLoadError::Misaligned);
        }
        if mem_size < file_data.len() {
            return Err(ElfLoadError::OutOfBounds);
        }

        ph_buf[i] = LoadSegment {
            vaddr,
            mem_size,
            file_data,
            file_off,
            flags,
            align,
        };
    }

    Ok(LoadPlan {
        entry: VirtAddr::new(elf.header().entry() as usize),
        segments: &ph_buf[..elf.program_headers().count()],
    })
}

/// Loads an ELF binary into the given address space using the provided architecture loader.
pub fn load_elf_into<'a, A: ElfLoader>(
    loader: &A,
    aspace: &mut A::AddrSpace,
    elf: &'a [u8],
    policy: LoadPolicy,
    seg_buf: &'a mut [LoadSegment<'a>],
) -> Result<LoadPlan<'a>, ElfLoadError> {
    let plan = build_load_plan(elf, policy, seg_buf)?;

    let page = loader.page_size();

    for seg in plan.segments.iter() {
        // enforce W^X if configured
        if !policy.allow_wx
            && seg.flags.contains(SegmentFlags::W)
            && seg.flags.contains(SegmentFlags::X)
        {
            return Err(ElfLoadError::Unsupported);
        }

        // round to pages (core decides)
        let map_start = seg.vaddr.align_down(page);
        let map_end = (seg.vaddr + seg.mem_size).align_up(page);

        loader
            .validate_user_range(aspace, map_start, (map_end - map_start).as_usize())
            .map_err(|_| ElfLoadError::AddressNotAllowed)?;

        // map pages with RW for loading (even if final flags are different, we'll fixup later)
        let load_flags = seg.flags | SegmentFlags::W;

        loader
            .map_anonymous(
                aspace,
                map_start,
                (map_end - map_start).as_usize(),
                load_flags,
            )
            .map_err(|_| ElfLoadError::MapFailed)?;

        // copy file bytes
        loader
            .copy_to_user(aspace, seg.vaddr, seg.file_data)
            .map_err(|_| ElfLoadError::CopyFailed)?;

        // zero .bss tail
        if seg.file_data.len() < seg.mem_size {
            let z_start = seg.vaddr + seg.file_data.len();
            let z_len = seg.mem_size - seg.file_data.len();
            loader
                .zero_user(aspace, z_start, z_len)
                .map_err(|_| ElfLoadError::ZeroFailed)?;
        }

        // drop write permission if not in original flags
        if seg.flags != load_flags {
            loader
                .protect_range(
                    aspace,
                    map_start,
                    (map_end - map_start).as_usize(),
                    seg.flags,
                )
                .map_err(|_| ElfLoadError::MapFailed)?;
        }
    }

    // finalize (icache/tlb discipline, etc.)
    loader
        .finalize_image(aspace, &[])
        .map_err(|_| ElfLoadError::Unsupported)?;

    Ok(plan)
}

/// Errors that can occur during ELF loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfLoadError {
    /// Invalid ELF format.
    BadElf(elf::ElfError),
    /// ELF binary is too small.
    Unsupported,
    /// ELF binary is too small.
    OutOfBounds,
    /// ELF segment has misaligned fields.
    Misaligned,
    /// Too many loadable segments.
    TooManySegments,
    /// Segment address range is not allowed in user space.
    AddressNotAllowed,
    /// Failed to map memory for a segment.
    MapFailed,
    /// Failed to copy segment data to user space.
    CopyFailed,
    /// Failed to zero segment memory in user space.
    ZeroFailed,
}

impl From<elf::ElfError> for ElfLoadError {
    fn from(err: elf::ElfError) -> Self {
        ElfLoadError::BadElf(err)
    }
}
