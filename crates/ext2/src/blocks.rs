#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct SuperBlock {
    inodes_count: u32,      // total number of inodes both used and free
    blocks_count: u32,      // total number of blocks both used, free and reserved
    r_blocks_count: u32,    // total number of blocks reserved for root
    free_blocks_count: u32, // total number of free blocks, including reserved
    free_inodes_count: u32, // total number of free inodes
    first_data_block: u32,  // first data block, ie. the id of the block containing the superblock
    log_block_size: u32,    // log2 of the block size
    log_frag_size: u32,     // log2 of the fragment size
    blocks_per_group: u32,  // number of blocks per group
    frags_per_group: u32,   // number of fragments per group
    inodes_per_group: u32,  // number of inodes per group
    mtime: u32,             // time of last mount
    wtime: u32,             // time of last write access to the file system
    mnt_count: u16,         // number of times the file system has been mounted since last fsck
    max_mnt_count: u16,     // maximum number of times the file system can be mounted before fsck
    magic: u16,             // magic number (should be 0xEF53)
    state: u16,             // file system state
    errors: u16,            // error behavior of the fs
    minor_rev_level: u16,   // minor revision level of the file system
    lastcheck: u32,         // time of last check
    checkinterval: u32,     // max. time between checks
    creator_os: u32,        // OS from which the file system was created
    rev_level: u32,         // revision level of the file system
    def_resuid: u16,        // default uid for reserved blocks
    def_resgid: u16,        // default gid for reserved blocks
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct BlockGroupDesc {
    block_bitmap: u32,      // block id of the first block of the block bitmap
    inode_bitmap: u32,      // block id of the first block of the inode bitmap
    inode_table: u32,       // block id of the first block of the inode table
    free_blocks_count: u16, // number of free blocks in the group
    free_inodes_count: u16, // number of free inodes in the group
    used_dirs_count: u16,   // number of inode allocated to dirs in the group
    pad: u16,
    reserved: [u8; 12],
}
