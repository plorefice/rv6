#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Inode {
    pub mode: u16,        // format of the file and access rights
    pub uid: u16,         // owner's user id
    pub size: u32,        // file size in bytes
    pub atime: u32,       // time of last access
    pub ctime: u32,       // time of creation
    pub mtime: u32,       // time of last modification
    pub dtime: u32,       // time of deletion
    pub gid: u16,         // owner's group id
    pub links_count: u16, // number of hard links to the file
    pub blocks: u32,      // number of blocks allocated for the file
    pub flags: u32,       // how the file can be accessed
    pub osd1: u32,        // OS-dependent 1
    pub block: [u32; 15], // block numbers pointing to data blocks
    pub generation: u32,  // generation number (used in NFS)
    pub file_acl: u32,    // always 0
    pub dir_acl: u32,     // always 0
    pub faddr: u32,       // fragment address
    pub osd2: [u8; 12],   // OS-dependent 2
}

impl Inode {
    pub const SIZE: usize = 128;

    pub const fn file_type(&self) -> InodeFileType {
        match self.mode & 0xF000 {
            0x1000 => InodeFileType::Fifo,
            0x2000 => InodeFileType::CharacterDevice,
            0x4000 => InodeFileType::Directory,
            0x6000 => InodeFileType::BlockDevice,
            0x8000 => InodeFileType::File,
            0xA000 => InodeFileType::Symlink,
            0xC000 => InodeFileType::Socket,
            _ => InodeFileType::Unknown,
        }
    }

    pub const fn is_dir(&self) -> bool {
        matches!(self.file_type(), InodeFileType::Directory)
    }

    pub const fn size(&self) -> u64 {
        self.size as u64
    }
}

impl From<[u8; Self::SIZE]> for Inode {
    fn from(value: [u8; Self::SIZE]) -> Self {
        // SAFETY: The array is guaranteed to be 128 bytes long
        unsafe { core::mem::transmute(value) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeFileType {
    Fifo,
    CharacterDevice,
    Directory,
    BlockDevice,
    File,
    Symlink,
    Socket,
    Unknown,
}
