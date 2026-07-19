use core::io::{self, Seek, SeekFrom};

use crate::{FileSystem, inode::Inode, io::Read};

pub struct File<'a, IO: Read + Seek> {
    pub(crate) fs: &'a mut FileSystem<IO>,
    pub(crate) inode: Inode,
    pub(crate) offset: u64,
}

impl<IO: Read + Seek> Read for File<'_, IO> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let remaining = self.inode.size() - self.offset;
        if remaining == 0 {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len() as u64, remaining) as usize;
        let bytes_read = self
            .fs
            .read_at(&self.inode, self.offset, &mut buf[..to_read])?;
        self.offset += bytes_read as u64;
        Ok(bytes_read)
    }
}

impl<IO: Read + Seek> Seek for File<'_, IO> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, io::Error> {
        let new_offset = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                let size = self.inode.size();
                if offset < 0 {
                    size.checked_sub((-offset) as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                } else {
                    size.checked_add(offset as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                }
            }
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    self.offset
                        .checked_sub((-offset) as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                } else {
                    self.offset
                        .checked_add(offset as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                }
            }
        };

        if new_offset > self.inode.size() {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        self.offset = new_offset;
        Ok(self.offset)
    }
}
