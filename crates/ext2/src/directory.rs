use core::{io::Seek, str::Utf8Error};

use crate::{error::Error, fs::FileSystem, inode::Inode, io::Read};

pub struct Dir<'a, IO: Read + Seek> {
    pub(crate) fs: &'a mut FileSystem<IO>,
    pub(crate) inode: Inode,
}

impl<IO: Read + Seek> Dir<'_, IO> {
    pub(crate) fn find(&mut self, name: &str) -> Result<Option<u32>, Error> {
        for entry in self.iter() {
            let entry = entry?;
            let file_name = entry.file_name().map_err(|_| Error::InvalidFilename)?;

            if file_name == name {
                return Ok(Some(entry.inode()));
            }
        }
        Ok(None)
    }

    pub fn iter(&mut self) -> DirIter<'_, IO> {
        DirIter {
            fs: self.fs,
            inode: self.inode,
            offset: 0,
            block: [0; 1024],
            block_offset: 0,
            block_valid: 0,
        }
    }
}

#[derive(Debug)]
pub struct DirEntry {
    inode: u32,
    name: [u8; 255],
    name_len: u8,
}

impl DirEntry {
    fn parse(buf: &[u8]) -> Result<(Option<Self>, u16), Error> {
        if buf.len() < 8 {
            return Err(Error::InvalidInput);
        }

        let inode = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let rec_len = u16::from_le_bytes(buf[4..6].try_into().unwrap());
        let name_len = buf[6] as usize;
        let _file_type = buf[7];

        if rec_len < 8 || rec_len as usize > buf.len() {
            return Err(Error::InvalidData);
        }
        if name_len > rec_len as usize - 8 {
            return Err(Error::InvalidData);
        }
        if name_len > 255 {
            return Err(Error::InvalidData);
        }

        if inode == 0 {
            return Ok((None, rec_len));
        }

        let mut name = [0u8; 255];
        name[..name_len].copy_from_slice(&buf[8..8 + name_len]);

        Ok((
            Some(Self {
                inode,
                name,
                name_len: name_len as u8,
            }),
            rec_len,
        ))
    }

    pub fn inode(&self) -> u32 {
        self.inode
    }

    pub fn file_name(&self) -> Result<&str, Utf8Error> {
        let name_bytes = &self.name[..self.name_len as usize];
        core::str::from_utf8(name_bytes)
    }
}

pub struct DirIter<'a, IO: Read + Seek> {
    fs: &'a mut FileSystem<IO>,
    inode: Inode,
    offset: u64,
    block: [u8; 1024],
    block_offset: usize,
    block_valid: usize,
}

impl<IO: Read + Seek> DirIter<'_, IO> {
    fn fill_block(&mut self) -> Result<(), Error> {
        let remaining = self.inode.size().saturating_sub(self.offset);
        if remaining == 0 {
            self.block_valid = 0;
            self.block_offset = 0;
            return Ok(());
        }

        let to_read = core::cmp::min(self.block.len() as u64, remaining) as usize;
        let n = self
            .fs
            .read_at(&self.inode, self.offset, &mut self.block[..to_read])?;
        self.block_valid = n;
        self.block_offset = 0;
        Ok(())
    }
}

impl<IO: Read + Seek> Iterator for DirIter<'_, IO> {
    type Item = Result<DirEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.offset >= self.inode.size() {
                return None;
            }

            if self.block_offset >= self.block_valid {
                if let Err(e) = self.fill_block() {
                    return Some(Err(e));
                }
                if self.block_valid == 0 {
                    return None;
                }
            }

            match DirEntry::parse(&self.block[self.block_offset..self.block_valid]) {
                Ok((entry, rec_len)) => {
                    self.block_offset += rec_len as usize;
                    self.offset += u64::from(rec_len);
                    if let Some(entry) = entry {
                        return Some(Ok(entry));
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct DirEntryRepr {
        inode: u32,
        rec_len: u16,
        name_len: u8,
        file_type: u8,
        name: Vec<u8>,
    }

    impl DirEntryRepr {
        fn to_bytes(&self) -> Vec<u8> {
            let mut buf = Vec::new();
            buf.extend_from_slice(&self.inode.to_le_bytes());
            buf.extend_from_slice(&self.rec_len.to_le_bytes());
            buf.push(self.name_len);
            buf.push(self.file_type);
            buf.extend_from_slice(&self.name);
            buf
        }
    }

    #[test]
    fn dir_entry_too_small() {
        let buf = [0u8; 7];
        let result = DirEntry::parse(&buf);
        assert!(matches!(result, Err(Error::InvalidInput)));
    }

    #[test]
    fn dir_entry_rec_len_too_small() {
        let buf = DirEntryRepr {
            inode: 1,
            rec_len: 7, // too small
            file_type: 1,
            name_len: 4,
            name: b"test".to_vec(),
        }
        .to_bytes();

        let result = DirEntry::parse(&buf);
        assert!(matches!(result, Err(Error::InvalidData)));
    }

    #[test]
    fn dir_entry_name_len_too_large() {
        let buf = DirEntryRepr {
            inode: 1,
            rec_len: 13,
            file_type: 1,
            name_len: 5, // too large
            name: b"test".to_vec(),
        }
        .to_bytes();

        let result = DirEntry::parse(&buf);
        assert!(matches!(result, Err(Error::InvalidData)));
    }

    #[test]
    fn dir_entry_name_len_too_large_for_rec_len() {
        let buf = DirEntryRepr {
            inode: 1,
            rec_len: 12,
            file_type: 1,
            name_len: 6, // too large for rec_len
            name: b"test".to_vec(),
        }
        .to_bytes();

        let result = DirEntry::parse(&buf);
        assert!(matches!(result, Err(Error::InvalidData)));
    }

    #[test]
    fn valid_dir_entry() {
        let buf = DirEntryRepr {
            inode: 1,
            rec_len: 12,
            file_type: 1,
            name_len: 4,
            name: b"test".to_vec(),
        }
        .to_bytes();

        let result = DirEntry::parse(&buf);
        assert!(matches!(result, Ok((Some(_), 12))));
    }
}
