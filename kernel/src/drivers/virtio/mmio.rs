//! VirtIO memory-mapped interface.

use core::mem::size_of;

use alloc::{boxed::Box, sync::Arc};
use fdt::Node;

use crate::{
    arch::{self, PAGE_SIZE},
    driver_info,
    drivers::{
        virtio::{InterruptStatus, VirtioBlkDev, VirtioDev, VirtioDriver, Virtq},
        Driver, DriverError,
    },
    mm::mmio::Regmap,
};

use super::Status;

driver_info! {
    type: VirtioMmio,
    of_match: ["virtio,mmio"],
}

/// Generic memory-mapped VirtIO device.
pub struct VirtioMmio {
    _driver: Box<dyn VirtioDriver>,
}

impl Driver for VirtioMmio {
    fn init<'d, 'fdt: 'd>(node: Node) -> Result<Arc<Self>, DriverError<'d>> {
        let (base, size) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        // SAFETY: assuming the node contains a valid regmap
        let regmap = unsafe { Regmap::new(base, size) };

        let dev = VirtioMmioDev { regmap };

        if &dev.magic().to_le_bytes() != b"virt" {
            return Err(DriverError::UnexpectedError("invalid virtio-mmio magic"));
        }

        if dev.version() != 1 {
            return Err(DriverError::UnexpectedError("invalid virtio-mmio version"));
        }

        let (dev_id, vendor_id) = dev.ids();

        // A device ID of 0 indicates a placeholder device
        if dev_id == 0 {
            return Err(DriverError::DeviceNotFound);
        }

        // Device initialization
        dev.set_guest_page_size(arch::PAGE_SIZE as u32);

        let driver = match dev_id {
            2 => Box::new(VirtioBlkDev::new(dev)),
            _ => todo!("unsupported virtio device"),
        };

        kprintln!("virtio-mmio: new device {vendor_id:x}:{dev_id:x} at 0x{base:x}");

        Ok(VirtioMmio { _driver: driver }.into())
    }
}

struct VirtioMmioDev {
    regmap: Regmap,
}

#[allow(unused)]
impl VirtioMmioDev {
    // Register offsets
    const MAGIC: usize = 0x00;
    const VERSION: usize = 0x04;
    const DEVICE_ID: usize = 0x08;
    const VENDOR_ID: usize = 0x0c;
    const DEVICE_FEATURES: usize = 0x10;
    const DEVICE_FEATURES_SEL: usize = 0x14;
    const DRIVER_FEATURES: usize = 0x20;
    const DRIVER_FEATURES_SEL: usize = 0x24;
    const GUEST_PAGE_SIZE: usize = 0x28;
    const QUEUE_SEL: usize = 0x30;
    const QUEUE_NUM_MAX: usize = 0x34;
    const QUEUE_NUM: usize = 0x38;
    const QUEUE_ALIGN: usize = 0x3c;
    const QUEUE_PFN: usize = 0x40;
    const QUEUE_NOTIFY: usize = 0x50;
    const INTERRUPT_STATUS: usize = 0x60;
    const INTERRUPT_ACK: usize = 0x64;
    const STATUS: usize = 0x70;
    const CONFIG: usize = 0x100;
}

impl VirtioMmioDev {
    fn magic(&self) -> u32 {
        self.regmap.read(Self::MAGIC)
    }

    fn version(&self) -> u32 {
        self.regmap.read(Self::VERSION)
    }

    fn ids(&self) -> (u32, u32) {
        (
            self.regmap.read(Self::DEVICE_ID),
            self.regmap.read(Self::VENDOR_ID),
        )
    }

    fn set_guest_page_size(&self, size: u32) {
        self.regmap.write(Self::GUEST_PAGE_SIZE, size);
    }
}

impl VirtioDev for VirtioMmioDev {
    fn status(&self) -> Status {
        Status::from_bits_retain(self.regmap.read::<u32>(Self::STATUS))
    }

    fn update_status(&self, status: Status) {
        self.regmap.write(Self::STATUS, self.status() | status);
    }

    fn read_device_features(&self, offset: u32) -> u32 {
        self.regmap.write(Self::DEVICE_FEATURES_SEL, offset);
        self.regmap.read(Self::DEVICE_FEATURES)
    }

    fn enable_device_features(&self, offset: u32, value: u32) {
        self.regmap.write(Self::DRIVER_FEATURES_SEL, offset);
        self.regmap.write(Self::DRIVER_FEATURES, value);
    }

    fn read_config<T: PartialEq>(&self, offset: u32) -> T {
        let mut old = self.regmap.read(Self::CONFIG + offset as usize);
        if size_of::<T>() <= size_of::<u32>() {
            return old;
        }

        loop {
            let new = self.regmap.read(Self::CONFIG + offset as usize);
            if old == new {
                break;
            }
            old = new;
        }
        old
    }

    fn allocate_virtq(&self, index: u32) -> Virtq {
        self.regmap.write(Self::QUEUE_SEL, index);

        if self.regmap.read::<u32>(Self::QUEUE_PFN) != 0 {
            panic!("queue in use");
        }

        let vq_num_max = self.regmap.read::<u32>(Self::QUEUE_NUM_MAX);

        let vq = Virtq::new(index, vq_num_max as u16);

        self.regmap.write(Self::QUEUE_NUM, vq_num_max);
        self.regmap.write(Self::QUEUE_ALIGN, PAGE_SIZE as u32);
        self.regmap.write(Self::QUEUE_PFN, vq.pfn());

        vq
    }

    fn notify(&self, index: u32) {
        self.regmap.write(Self::QUEUE_NOTIFY, index);
    }

    fn interrupts(&self) -> InterruptStatus {
        InterruptStatus::from_bits_retain(self.regmap.read(Self::INTERRUPT_STATUS))
    }

    fn clear_interrupts(&self, status: InterruptStatus) {
        self.regmap.write(Self::INTERRUPT_ACK, status);
    }
}
