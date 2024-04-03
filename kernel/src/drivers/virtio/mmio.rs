//! VirtIO memory-mapped interface.

use alloc::sync::Arc;
use fdt::Node;

use crate::{
    driver_info,
    drivers::{Driver, DriverError},
    mm::mmio::Regmap,
};

driver_info! {
    type: VirtioMmio,
    of_match: ["virtio,mmio"],
}

/// Generic memory-mapped VirtIO device.
pub struct VirtioMmio {
    _regmap: Regmap,
}

impl Driver for VirtioMmio {
    fn init<'d, 'fdt: 'd>(node: Node) -> Result<Arc<Self>, DriverError<'d>> {
        let (base, size) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        // SAFETY: assuming the node contains a valid regmap
        let regmap = unsafe { Regmap::new(base, size) };

        let magic = regmap.read::<u32>(0x00).to_le_bytes();
        if &magic != b"virt" {
            return Err(DriverError::UnexpectedError("invalid virtio-mmio magic"));
        }

        let version = regmap.read::<u32>(0x04);
        if version != 1 {
            return Err(DriverError::UnexpectedError("invalid virtio-mmio version"));
        }

        let dev_id = regmap.read::<u32>(0x08);
        let vendor_id = regmap.read::<u32>(0x0c);

        // A device ID of 0 indicates a placeholder device
        if dev_id == 0 {
            return Err(DriverError::DeviceNotFound);
        }

        kprintln!("virtio-mmio: new device {vendor_id:x}:{dev_id:x} at 0x{base:x}");

        Ok(Arc::new(VirtioMmio { _regmap: regmap }))
    }
}
