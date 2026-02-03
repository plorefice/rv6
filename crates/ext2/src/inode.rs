#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Inode {
    mode: u16,        // format of the file and access rights
    uid: u16,         // owner's user id
    size: u32,        // file size in bytes
    atime: u32,       // time of last access
    ctime: u32,       // time of creation
    mtime: u32,       // time of last modification
    dtime: u32,       // time of deletion
    gid: u16,         // owner's group id
    links_count: u16, // number of hard links to the file
    blocks: u32,      // number of blocks allocated for the file
    flags: u32,       // how the file can be accessed
    osd1: u32,        // OS-dependent 1
    block: [u32; 15], // block numbers pointing to data blocks
    generation: u32,  // generation number (used in NFS)
    file_acl: u32,    // always 0
    dir_acl: u32,     // always 0
    faddr: u32,       // fragment address
    osd2: [u8; 12],   // OS-dependent 2
}
