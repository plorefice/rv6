//! VirtIO subsystem.

use bitflags::bitflags;
pub use blkdev::*;
pub use mmio::*;

use crate::{
    drivers::virtio::virtq::Virtq,
    mm::dma::{DmaAllocError, DmaObject, DmaSafe},
};

mod blkdev;
mod mmio;
mod virtq;

/// Abstraction over VirtIO devices with PCI and MMIO interfaces.
pub trait VirtioDev {
    /// Reads the device's status register.
    fn status(&self) -> Status;

    /// Writes the specified bitmask to the device's status register.
    fn update_status(&self, status: Status);

    /// Reads flags representing features the device supports.
    ///
    /// Reading from this register returns 32 consecutive flag bits, the least significant bit
    /// depending on the `offset`, e.g. feature bits 0 to 31 if `offset` is set to 0, features
    /// bits 32 to 63 if `offset` is set to 1 and so on.
    fn read_device_features(&self, offset: u32) -> u32;

    /// Writes flags representing device features understood and activated by the driver.
    ///
    /// See [`VirtioDev::read_device_features`] for the `offset` format.
    fn enable_device_features(&self, offset: u32, value: u32);

    /// Reads a configuration field from the device.
    ///
    /// The offset is expressed in bytes.
    fn read_config<T: PartialEq>(&self, offset: u32) -> T;

    /// Allocates guest memory suitable for DMA operations.
    fn allocate_guest_mem<T: DmaSafe>(&self, val: T) -> Result<DmaObject<T>, DmaAllocError>;

    /// Allocates and configures the specified virtqueue.
    fn allocate_virtq(&self, index: u32) -> Virtq;

    /// Notities the device that new buffers are available in the selected virtqueue.
    fn notify(&self, index: u32);

    /// Reads the interrupt status register.
    fn interrupts(&self) -> InterruptStatus;

    /// Notifies the device that the events causing the interrupt have been handled.
    fn clear_interrupts(&self, status: InterruptStatus);
}

/// Driver for a specific VirtIO peripheral.
pub trait VirtioDriver {}

bitflags! {
    /// VirtIO status register bits.
    #[derive(Debug, Clone, Copy)]
    pub struct Status: u32 {
        /// The guest OS has found the device and recognized it as a valid virtio device
        const ACKNOWLEDGE = 1;
        /// The guest OS knows how to drive the device
        const DRIVER = 2;
        /// The driver is set up and ready to drive the device
        const DRIVER_OK = 4;
        /// The device has experienced an error from which it can’t recover
        const DEVICE_NEEDS_RESET = 64;
        /// Something went wrong in the guest, and it has given up on the device
        const FAILED = 128;
    }
}

bitflags! {
    /// VirtIO interrupt status register bits.
    #[derive(Debug, Clone, Copy)]
    pub struct InterruptStatus: u32 {
        /// The device has used a buffer in at least one of the active virtual queues
        const USED_BUFFER = 1;
        /// The configuration of the device has changed
        const CONFIG_CHANGED = 2;
    }
}
