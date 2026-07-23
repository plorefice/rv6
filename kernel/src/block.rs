//! Block device interface and error types.
//!
//! A block device is a device that provides access to data in fixed-size blocks (sectors).
//! This module defines the [`BlockDev`] trait, which represents a synchronous block device,
//! and the [`BlockIoError`] type, which represents errors that can occur during block I/O operations.

use core::{
    error::Error,
    fmt,
    io::{self, SeekFrom},
};

use alloc::{sync::Arc, vec::Vec};

use crate::sync::IrqSpinLock;

/// An error that can occur during block I/O operations.
#[derive(Debug)]
pub enum BlockIoError {
    /// Device reported an I/O error.
    Io(io::Error),
}

impl fmt::Display for BlockIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockIoError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl Error for BlockIoError {}

impl From<BlockIoError> for io::Error {
    fn from(e: BlockIoError) -> Self {
        match e {
            BlockIoError::Io(io_err) => io_err,
        }
    }
}

/// A synchronous block device.
///
/// All offsets and sizes are in **sectors** of [`BlockDev::sector_size`] bytes
/// (typically 512 bytes). Buffers passed in and out must be a multiple of the sector size.
pub trait BlockDev: Send + Sync {
    /// Returns the size of a sector in bytes.
    fn sector_size(&self) -> usize {
        512
    }

    /// Returns the total number of sectors on this device.
    fn capacity(&self) -> u64;

    /// Reads `count` sectors starting at `start` into `buf`.
    ///
    /// Returns an error if the read fails or if `buf` is not exactly `count * sector_size()`
    /// bytes long.
    fn read_sectors(&self, start: u64, count: u64, buf: &mut [u8]) -> Result<(), BlockIoError>;

    /// Writes `count` sectors starting at `start` from `buf`.
    ///
    /// Returns an error if the write fails or if `buf` is not exactly `count * sector_size()`
    /// bytes long.
    fn write_sectors(&self, start: u64, count: u64, buf: &[u8]) -> Result<(), BlockIoError>;

    /// Flushes any pending writes to the device.
    ///
    /// Default implementation does nothing.
    fn flush(&self) -> Result<(), BlockIoError> {
        Ok(())
    }

    /// Returns true if the device is read-only.
    fn readonly(&self) -> bool {
        false
    }
}

impl dyn BlockDev {
    /// Reads a single sector from the device into `buf`.
    pub fn read_sector(&self, sector: u64, buf: &mut [u8]) -> Result<(), BlockIoError> {
        self.read_sectors(sector, 1, buf)
    }

    /// Writes a single sector to the device from `buf`.
    pub fn write_sector(&self, sector: u64, buf: &[u8]) -> Result<(), BlockIoError> {
        self.write_sectors(sector, 1, buf)
    }
}

/// A unique identifier for a registered block device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockDevId(u32);

/// A table of block devices, indexed by device number.
pub struct BlockDevTable {
    devices: Vec<Arc<dyn BlockDev>>,
}

impl Default for BlockDevTable {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockDevTable {
    /// Creates a new, empty block device table.
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Adds a block device to the table and returns its unique identifier.
    pub fn register(&mut self, dev: Arc<dyn BlockDev>) -> BlockDevId {
        let devno = self.devices.len() as u32;
        self.devices.push(dev);
        BlockDevId(devno)
    }

    /// Returns a reference to the block device with the given identifier, or `None` if no such device exists.
    pub fn get(&self, id: BlockDevId) -> Option<&Arc<dyn BlockDev>> {
        self.devices.get(id.0 as usize)
    }

    /// Returns an iterator over all registered block devices.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn BlockDev>> {
        self.devices.iter()
    }
}

/// Global block device table, protected by a spinlock for thread-safe access.
pub static BLOCK_DEVS: IrqSpinLock<BlockDevTable> = IrqSpinLock::new(BlockDevTable::new());

/// A byte-oriented cursor for reading from a block device.
///
/// This struct provides a convenient way to read data from a block device in a byte-oriented manner,
/// while internally managing sector-aligned reads and buffering.
pub struct BlockDevCursor {
    dev: Arc<dyn BlockDev>, // The block device being read from
    pos: u64,               // Current byte position in the device
    sector: Vec<u8>,        // Buffer for a single sector worth of data
    sector_index: u64,      // Index of the currently buffered sector
}

impl BlockDevCursor {
    /// Creates a new `BlockDevCursor` for the given block device.
    pub fn new(dev: Arc<dyn BlockDev>) -> Self {
        let sector_size = dev.sector_size();

        Self {
            dev,
            pos: 0,
            sector: vec![0; sector_size],
            sector_index: u64::MAX, // Invalid index to force initial read
        }
    }
}

impl ext2::Read for BlockDevCursor {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let capacity = self.dev.capacity() * self.dev.sector_size() as u64;
        if self.pos >= capacity || buf.is_empty() {
            return Ok(0); // EOF
        }

        let sector_index = self.pos / self.dev.sector_size() as u64;
        let sector_offset = (self.pos % self.dev.sector_size() as u64) as usize;

        // Read the current sector into the buffer if needed
        if self.sector_index != sector_index {
            self.dev.read_sector(sector_index, &mut self.sector)?;
            self.sector_index = sector_index;
        }

        // Calculate how many bytes we can read from the current sector
        let bytes_to_read = core::cmp::min(buf.len(), self.sector.len() - sector_offset);
        let bytes_to_read = core::cmp::min(bytes_to_read, (capacity - self.pos) as usize);
        buf[..bytes_to_read]
            .copy_from_slice(&self.sector[sector_offset..sector_offset + bytes_to_read]);
        self.pos += bytes_to_read as u64;

        Ok(bytes_to_read)
    }
}

impl io::Seek for BlockDevCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let capacity = self.dev.capacity() * self.dev.sector_size() as u64;

        let new_pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                if offset < 0 {
                    capacity
                        .checked_sub((-offset) as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                } else {
                    capacity
                        .checked_add(offset as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                }
            }
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    self.pos
                        .checked_sub((-offset) as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                } else {
                    self.pos
                        .checked_add(offset as u64)
                        .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?
                }
            }
        };

        if new_pos > capacity {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        self.pos = new_pos;
        Ok(self.pos)
    }
}
