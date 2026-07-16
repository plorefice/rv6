//! Block device interface and error types.
//!
//! A block device is a device that provides access to data in fixed-size blocks (sectors).
//! This module defines the [`BlockDev`] trait, which represents a synchronous block device,
//! and the [`BlockIoError`] type, which represents errors that can occur during block I/O operations.

use core::{error::Error, fmt, io};

use alloc::{sync::Arc, vec::Vec};

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
pub static BLOCK_DEVS: spin::Mutex<BlockDevTable> = spin::Mutex::new(BlockDevTable::new());
