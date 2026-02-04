//! Minimal ELF64 little-endian parser.
//! Supports reading the ELF header and iterating program headers.
//!
//! Safety model:
//! - Uses bounds-checked slicing + manual LE decoding.
//! - No unsafe required.

#![no_std]

pub mod abi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    NotElf64,
    NotLittleEndian,
    BadHeaderSize,
    BadPhEntSize,
    OutOfBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64Header {
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// ELF identification constants.
const EI_NIDENT: usize = 16;
const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

pub struct Elf64<'a> {
    data: &'a [u8],
    hdr: Elf64Header,
}

impl<'a> Elf64<'a> {
    /// Parse ELF header from `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        // Need at least the full ELF64 header (64 bytes).
        if data.len() < 64 {
            return Err(ElfError::TooSmall);
        }

        // e_ident
        let ident = get_range(data, 0, EI_NIDENT)?;
        if ident[0..4] != ELFMAG {
            return Err(ElfError::BadMagic);
        }
        if ident[4] != ELFCLASS64 {
            return Err(ElfError::NotElf64);
        }
        if ident[5] != ELFDATA2LSB {
            return Err(ElfError::NotLittleEndian);
        }

        // Fixed offsets for ELF64 header fields (System V ABI).
        // Layout:
        // 0x00 e_ident[16]
        // 0x10 e_type (2)
        // 0x12 e_machine (2)
        // 0x14 e_version (4)
        // 0x18 e_entry (8)
        // 0x20 e_phoff (8)
        // 0x28 e_shoff (8)
        // 0x30 e_flags (4)
        // 0x34 e_ehsize (2)
        // 0x36 e_phentsize (2)
        // 0x38 e_phnum (2)
        // 0x3A e_shentsize (2)
        // 0x3C e_shnum (2)
        // 0x3E e_shstrndx (2)

        let e_type = read_u16_le(get_range(data, 0x10, 2)?)?;
        let e_machine = read_u16_le(get_range(data, 0x12, 2)?)?;
        let e_version = read_u32_le(get_range(data, 0x14, 4)?)?;
        let e_entry = read_u64_le(get_range(data, 0x18, 8)?)?;
        let e_phoff = read_u64_le(get_range(data, 0x20, 8)?)?;
        let e_shoff = read_u64_le(get_range(data, 0x28, 8)?)?;
        let e_flags = read_u32_le(get_range(data, 0x30, 4)?)?;
        let e_ehsize = read_u16_le(get_range(data, 0x34, 2)?)?;
        let e_phentsize = read_u16_le(get_range(data, 0x36, 2)?)?;
        let e_phnum = read_u16_le(get_range(data, 0x38, 2)?)?;
        let e_shentsize = read_u16_le(get_range(data, 0x3A, 2)?)?;
        let e_shnum = read_u16_le(get_range(data, 0x3C, 2)?)?;
        let e_shstrndx = read_u16_le(get_range(data, 0x3E, 2)?)?;

        // Sanity checks.
        if e_ehsize != 64 {
            return Err(ElfError::BadHeaderSize);
        }
        if e_phnum != 0 && e_phentsize != 56 {
            return Err(ElfError::BadPhEntSize);
        }

        // Validate program header table bounds, if present.
        if e_phnum != 0 {
            let phoff = usize::try_from(e_phoff).map_err(|_| ElfError::OutOfBounds)?;
            let entsz = usize::from(e_phentsize);
            let num = usize::from(e_phnum);
            let bytes = entsz.checked_mul(num).ok_or(ElfError::OutOfBounds)?;
            get_range(data, phoff, bytes)?; // bounds check
        }

        Ok(Self {
            data,
            hdr: Elf64Header {
                e_type,
                e_machine,
                e_version,
                e_entry,
                e_phoff,
                e_shoff,
                e_flags,
                e_ehsize,
                e_phentsize,
                e_phnum,
                e_shentsize,
                e_shnum,
                e_shstrndx,
            },
        })
    }

    pub fn header(&self) -> &Elf64Header {
        &self.hdr
    }

    pub fn program_headers(&self) -> ProgramHeaderIter<'a> {
        let phoff = self.hdr.e_phoff as usize;
        ProgramHeaderIter {
            data: self.data,
            off: phoff,
            idx: 0,
            count: self.hdr.e_phnum as usize,
            entsz: self.hdr.e_phentsize as usize,
        }
    }

    pub fn segment_data(&self, ph: &Elf64Phdr) -> Result<&'a [u8], ElfError> {
        let off = usize::try_from(ph.p_offset).map_err(|_| ElfError::OutOfBounds)?;
        let filesz = usize::try_from(ph.p_filesz).map_err(|_| ElfError::OutOfBounds)?;
        get_range(self.data, off, filesz)
    }
}

impl Elf64Header {
    pub fn is_executable(&self) -> bool {
        self.e_type == abi::ET_EXEC
    }
}

impl Elf64Phdr {
    pub fn is_load(&self) -> bool {
        self.p_type == abi::PT_LOAD
    }

    pub fn is_readable(&self) -> bool {
        (self.p_flags & abi::PF_R) != 0
    }

    pub fn is_writable(&self) -> bool {
        (self.p_flags & abi::PF_W) != 0
    }

    pub fn is_executable(&self) -> bool {
        (self.p_flags & abi::PF_X) != 0
    }
}

pub struct ProgramHeaderIter<'a> {
    data: &'a [u8],
    off: usize,
    idx: usize,
    count: usize,
    entsz: usize,
}

impl<'a> ProgramHeaderIter<'a> {
    fn parse_one(&self, ph: &[u8]) -> Result<Elf64Phdr, ElfError> {
        // ELF64 Phdr is 56 bytes:
        // 0x00 p_type   (4)
        // 0x04 p_flags  (4)
        // 0x08 p_offset (8)
        // 0x10 p_vaddr  (8)
        // 0x18 p_paddr  (8)
        // 0x20 p_filesz (8)
        // 0x28 p_memsz  (8)
        // 0x30 p_align  (8)
        if ph.len() < 56 {
            return Err(ElfError::TooSmall);
        }
        Ok(Elf64Phdr {
            p_type: read_u32_le(&ph[0x00..0x04])?,
            p_flags: read_u32_le(&ph[0x04..0x08])?,
            p_offset: read_u64_le(&ph[0x08..0x10])?,
            p_vaddr: read_u64_le(&ph[0x10..0x18])?,
            p_paddr: read_u64_le(&ph[0x18..0x20])?,
            p_filesz: read_u64_le(&ph[0x20..0x28])?,
            p_memsz: read_u64_le(&ph[0x28..0x30])?,
            p_align: read_u64_le(&ph[0x30..0x38])?,
        })
    }
}

impl<'a> Iterator for ProgramHeaderIter<'a> {
    type Item = Result<Elf64Phdr, ElfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.count {
            return None;
        }
        let start = self.off + self.idx * self.entsz;
        self.idx += 1;

        Some(get_range(self.data, start, self.entsz).and_then(|ph| self.parse_one(ph)))
    }
}

fn read_u16_le(b: &[u8]) -> Result<u16, ElfError> {
    if b.len() < 2 {
        return Err(ElfError::TooSmall);
    }
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32_le(b: &[u8]) -> Result<u32, ElfError> {
    if b.len() < 4 {
        return Err(ElfError::TooSmall);
    }
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u64_le(b: &[u8]) -> Result<u64, ElfError> {
    if b.len() < 8 {
        return Err(ElfError::TooSmall);
    }
    Ok(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

fn get_range(data: &[u8], off: usize, len: usize) -> Result<&[u8], ElfError> {
    data.get(off..off + len).ok_or(ElfError::OutOfBounds)
}
