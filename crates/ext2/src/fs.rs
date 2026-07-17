use core::io::{Seek, SeekFrom};

use crate::{
    blocks::BlockGroupDesc, directory::Dir, error::Error, file::File, inode::Inode, io::Read,
    superblock::SuperBlock,
};

pub struct FileSystem<IO: Read + Seek> {
    disk: IO,
    superblock: SuperBlock,
}

pub trait IntoStorage<T: Read + Seek> {
    fn into_storage(self) -> T;
}

impl<T: Read + Seek> IntoStorage<T> for T {
    fn into_storage(self) -> T {
        self
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Read + std::io::Seek> IntoStorage<crate::io::StdIoWrapper<T>> for T {
    fn into_storage(self) -> crate::io::StdIoWrapper<T> {
        crate::io::StdIoWrapper::from(self)
    }
}

impl<IO: Read + Seek> FileSystem<IO> {
    const ROOT_INODE: u32 = 2;

    pub fn mount<T: IntoStorage<IO>>(disk: T) -> Result<Self, Error> {
        let mut disk = disk.into_storage();

        let mut buf = [0; SuperBlock::SIZE];
        disk.seek(SeekFrom::Start(1024))?; // Superblock is located at offset 1024
        disk.read_exact(&mut buf)?;

        let superblock = SuperBlock::from(buf);
        if superblock.magic != 0xEF53 {
            return Err(Error::BadMagic);
        }

        disk.seek(SeekFrom::Start(0))?;

        Ok(Self { disk, superblock })
    }

    pub fn open<'a>(&'a mut self, path: &str) -> Result<File<'a, IO>, Error> {
        let inode = self.resolve_path(path)?;
        if inode.is_dir() {
            return Err(Error::IsADirectory);
        }

        Ok(File {
            fs: self,
            inode,
            offset: 0,
        })
    }

    pub fn open_dir<'a>(&'a mut self, path: &str) -> Result<Dir<'a, IO>, Error> {
        let inode = self.resolve_path(path)?;
        if !inode.is_dir() {
            return Err(Error::NotADirectory);
        }

        Ok(Dir { fs: self, inode })
    }

    fn resolve_path(&mut self, path: &str) -> Result<Inode, Error> {
        let mut current_inode = self.read_inode(Self::ROOT_INODE)?;

        for component in path.split('/') {
            if component.is_empty() || component == "." {
                continue;
            }

            // TODO: Handle ".." to navigate to the parent directory
            if component == ".." {
                return Err(Error::Unsupported);
            }

            if !current_inode.is_dir() {
                return Err(Error::NotADirectory);
            }

            let mut dir = Dir {
                fs: self,
                inode: current_inode,
            };
            match dir.find(component)? {
                Some(inum) => current_inode = self.read_inode(inum)?,
                None => return Err(Error::NotFound),
            }
        }

        Ok(current_inode)
    }

    fn read_inode(&mut self, inum: u32) -> Result<Inode, Error> {
        let inode_group = (inum - 1) / self.inodes_per_group();
        let group_desc = self.read_group_desc(inode_group)?;

        let inode_table_block = group_desc.inode_table as u64;
        let inode_table_offset = inode_table_block * self.block_size() as u64;
        let inode_index = (inum - 1) % self.inodes_per_group();
        let inode_offset = inode_table_offset + (inode_index as u64 * self.inode_size() as u64);

        let mut inode_buf = [0; Inode::SIZE];
        self.disk.seek(SeekFrom::Start(inode_offset))?;
        self.disk.read_exact(&mut inode_buf)?;

        Ok(Inode::from(inode_buf))
    }

    fn read_group_desc(&mut self, group: u32) -> Result<BlockGroupDesc, Error> {
        let gdt_offset = self.gdt_offset();
        let gdt_entry_offset = gdt_offset + (group as u64 * BlockGroupDesc::SIZE as u64);

        let mut buf = [0; BlockGroupDesc::SIZE];
        self.disk.seek(SeekFrom::Start(gdt_entry_offset))?;
        self.disk.read_exact(&mut buf)?;

        Ok(BlockGroupDesc::from(buf))
    }

    fn map_block(&mut self, inode: &Inode, block_index: u32) -> Result<u32, Error> {
        if block_index < 12 {
            Ok(inode.block[block_index as usize])
        } else {
            let indirect_block_index = block_index - 12;
            let indirect_block_num = inode.block[12];
            if indirect_block_num == 0 {
                return Err(Error::InvalidData);
            }

            let mut buf = [0; 4096]; // enough for any block size
            self.read_block(indirect_block_num, &mut buf)?;

            let entry_offset = indirect_block_index as usize * 4;
            if entry_offset >= self.block_size() as usize {
                return Err(Error::Unsupported); // TODO: Handle double and triple indirect blocks
            }

            let block_num =
                u32::from_le_bytes(buf[entry_offset..entry_offset + 4].try_into().unwrap());
            Ok(block_num)
        }
    }

    fn read_block(&mut self, block_num: u32, buf: &mut [u8]) -> Result<(), Error> {
        let block_size = self.block_size() as usize;
        if buf.len() < block_size {
            return Err(Error::InvalidInput);
        }

        let offset = block_num as u64 * block_size as u64;
        self.disk.seek(SeekFrom::Start(offset))?;
        self.disk.read_exact(&mut buf[..block_size])?;

        Ok(())
    }

    pub(crate) fn read_at(
        &mut self,
        inode: &Inode,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        let block_size = self.block_size() as u64;
        let start_block_index = (offset / block_size) as u32;
        let end_block_index = ((offset + buf.len() as u64 - 1) / block_size) as u32;

        let mut total_bytes_read = 0;

        for block_index in start_block_index..=end_block_index {
            let block_num = self.map_block(inode, block_index)?;
            let block_offset = block_num as u64 * block_size;

            let read_offset = if block_index == start_block_index {
                offset % block_size
            } else {
                0
            };

            let bytes_to_read = if block_index == end_block_index {
                let end = offset + buf.len() as u64;
                let end_in_block = match end % block_size {
                    0 => block_size,
                    n => n,
                };
                (end_in_block - read_offset) as usize
            } else {
                (block_size - read_offset) as usize
            };

            self.disk
                .seek(SeekFrom::Start(block_offset + read_offset))?;
            self.disk
                .read_exact(&mut buf[total_bytes_read..total_bytes_read + bytes_to_read])?;

            total_bytes_read += bytes_to_read;
        }

        Ok(total_bytes_read)
    }

    const fn block_size(&self) -> u32 {
        1024 << self.superblock.log_block_size
    }

    const fn inodes_per_group(&self) -> u32 {
        self.superblock.inodes_per_group
    }

    const fn gdt_offset(&self) -> u64 {
        let block_size = self.block_size() as u64;
        let first_data_block = self.superblock.first_data_block as u64;
        (first_data_block + 1) * block_size
    }

    const fn inode_size(&self) -> u16 {
        if self.superblock.rev_level == 0 {
            128
        } else {
            self.superblock.inode_size
        }
    }
}
