#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SuperBlock {
    pub inodes_count: u32,       // total number of inodes both used and free
    pub blocks_count: u32,       // total number of blocks both used, free and reserved
    pub r_blocks_count: u32,     // total number of blocks reserved for root
    pub free_blocks_count: u32,  // total number of free blocks, including reserved
    pub free_inodes_count: u32,  // total number of free inodes
    pub first_data_block: u32, // first data block, ie. the id of the block containing the superblock
    pub log_block_size: u32,   // log2 of the block size
    pub log_frag_size: u32,    // log2 of the fragment size
    pub blocks_per_group: u32, // number of blocks per group
    pub frags_per_group: u32,  // number of fragments per group
    pub inodes_per_group: u32, // number of inodes per group
    pub mtime: u32,            // time of last mount
    pub wtime: u32,            // time of last write access to the file system
    pub mnt_count: u16,        // number of times the file system has been mounted since last fsck
    pub max_mnt_count: u16,    // maximum number of times the file system can be mounted before fsck
    pub magic: u16,            // magic number (should be 0xEF53)
    pub state: u16,            // file system state
    pub errors: u16,           // error behavior of the fs
    pub minor_rev_level: u16,  // minor revision level of the file system
    pub lastcheck: u32,        // time of last check
    pub checkinterval: u32,    // max. time between checks
    pub creator_os: u32,       // OS from which the file system was created
    pub rev_level: u32,        // revision level of the file system
    pub def_resuid: u16,       // default uid for reserved blocks
    pub def_resgiqd: u16,      // default gid for reserved blocks
    pub first_ino: u32,        // first non-reserved inode
    pub inode_size: u16,       // size of an inode structure
    pub block_group_nr: u16,   // block group number of this superblock
    pub feature_compat: u32,   // compatible feature set
    pub feature_incompat: u32, // incompatible feature set
    pub feature_ro_compat: u32, // read-only compatible feature set
    pub uuid: [u8; 16],        // 128-bit unique identifier for the file system
    pub volume_name: [u8; 16], // volume name
    pub last_mounted: [u8; 64], // directory where the file system was last mounted
    pub algo_bitmap: u32,      // for compression
    pub prealloc_blocks: u8,   // number of blocks to preallocate for files
    pub prealloc_dir_blocks: u8, // number of blocks to preallocate for directories
    _pad1: [u8; 2],            // padding to align to 4 bytes
    pub journal_uuid: [u8; 16], // UUID of the journal superblock
    pub journal_inum: u32,     // inode number of the journal
    pub journal_dev: u32,      // device number of the journal file
    pub last_orphan: u32,      // start of the list of orphaned inodes
    pub hash_seed: [u32; 4],   // HTREE hash seed
    pub def_hash_version: u8,  // default hash version to use
    _pad2: [u8; 3],            // padding to align to 4 bytes
    pub default_mount_opts: u32, // default mount options
    pub first_meta_bg: u32,    // block group number of the first meta block
    _pad3: [u8; 760],          // padding to make the struct 1024 bytes
}

impl SuperBlock {
    pub const SIZE: usize = 1024;
}

impl From<[u8; Self::SIZE]> for SuperBlock {
    fn from(value: [u8; Self::SIZE]) -> Self {
        // SAFETY: The array is guaranteed to be 1024 bytes long
        unsafe { core::mem::transmute(value) }
    }
}
