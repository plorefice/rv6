#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct BlockGroupDesc {
    pub block_bitmap: u32,      // block id of the first block of the block bitmap
    pub inode_bitmap: u32,      // block id of the first block of the inode bitmap
    pub inode_table: u32,       // block id of the first block of the inode table
    pub free_blocks_count: u16, // number of free blocks in the group
    pub free_inodes_count: u16, // number of free inodes in the group
    pub used_dirs_count: u16,   // number of inode allocated to dirs in the group
    pad: u16,
    reserved: [u8; 12],
}

impl BlockGroupDesc {
    pub const SIZE: usize = 32;
}

impl From<[u8; Self::SIZE]> for BlockGroupDesc {
    fn from(value: [u8; Self::SIZE]) -> Self {
        // SAFETY: The array is guaranteed to be 32 bytes long
        unsafe { core::mem::transmute(value) }
    }
}
