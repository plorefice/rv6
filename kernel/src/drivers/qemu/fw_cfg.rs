//! QEMU Firmware Configuration (fw_cfg) Device Driver
//!
//! This driver provides an interface to the QEMU Firmware Configuration (fw_cfg) device, which
//! allows the guest operating system to access configuration data provided by the QEMU hypervisor.
//! The fw_cfg device is typically used to retrieve information about the virtual machine's
//! configuration, such as the presence of specific files or features.

use core::{mem, num::NonZeroUsize};

use bitflags::bitflags;
use fdt::Node;

use crate::{
    driver_info,
    drivers::{Driver, DriverCtx, DriverError},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        dma::{self, DmaAllocator, DmaAllocatorExt, DmaDirection, DmaSafe},
        mmio::{self, IoMapper, IoMapping},
    },
};

driver_info! {
    type: QemuFwCfg,
    of_match: ["qemu,fw-cfg-mmio"],
}

/// QEMU Firmware Configuration (fw_cfg) device driver.
pub struct QemuFwCfg {
    regmap: IoMapping,
}

impl Driver for QemuFwCfg {
    fn init<'d, 'fdt: 'd>(_: &DriverCtx, node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>> {
        let (base, size) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let pa_base = PhysAddr::new(base as usize);
        let size =
            NonZeroUsize::new(size as usize).ok_or(DriverError::InvalidPropertyValue("reg"))?;

        let regmap = mmio::mapper().iomap(pa_base, size)?;

        let fw_cfg = Self { regmap };

        if fw_cfg.signature() != *b"QEMU" {
            return Err(DriverError::UnexpectedError("Invalid signature"));
        }
        if fw_cfg.features() & 0x2 == 0 {
            return Err(DriverError::UnexpectedError("DMA not supported"));
        }

        kprintln!("QEMU FW_CFG: configured");

        let ramfb_file = fw_cfg
            .find_file("etc/ramfb")
            .ok_or(DriverError::DeviceNotFound)?;

        kprintln!(
            "QEMU FW_CFG: found ramfb file, selector = {}, size = {}",
            ramfb_file.selector,
            ramfb_file.size
        );

        Ok(())
    }
}

impl QemuFwCfg {
    const DMA_ACCESS_REG: usize = 0x10;

    const FW_CFG_SIGNATURE: u16 = 0x0000;
    const FW_CFG_ID: u16 = 0x0001;
    const FW_CFG_FILE_DIR: u16 = 0x0019;

    fn signature(&self) -> [u8; 4] {
        self.read(dma::allocator(), Some(Self::FW_CFG_SIGNATURE))
            .unwrap()
    }

    fn features(&self) -> u32 {
        self.read(dma::allocator(), Some(Self::FW_CFG_ID)).unwrap()
    }

    fn find_file(&self, name: &str) -> Option<FwCfgFile> {
        let dma = dma::allocator();
        let count = self
            .read::<u32>(dma, Some(Self::FW_CFG_FILE_DIR))
            .unwrap()
            .swap_bytes();

        for _ in 0..count {
            let mut file = self.read::<FwCfgFile>(dma, None).unwrap(); // read the next file entry

            let file_name = match core::str::from_utf8(&file.name) {
                Ok(s) => s.trim_end_matches('\0'),
                Err(_) => continue,
            };

            if file_name == name {
                // Fix endianness of the fields in the file entry
                file.size = file.size.swap_bytes();
                file.selector = file.selector.swap_bytes();

                return Some(file);
            }
        }

        None
    }

    fn read<T: DmaSafe>(
        &self,
        dma: &impl DmaAllocator,
        selector: Option<u16>,
    ) -> Result<T, DriverError<'static>> {
        // TODO: there are several dma object leaks in this function due to early returns on error.

        let obj = dma.alloc_uninit::<T>()?;

        let access = dma.alloc(FwCfgDmaAccess::new(
            selector
                .map(FwCfgDmaAccessControl::select)
                .unwrap_or(FwCfgDmaAccessControl::empty())
                | FwCfgDmaAccessControl::READ,
            mem::size_of::<T>() as u32,
            obj.dma_addr().into(),
        ))?;

        dma.sync_object_for_device(&access, DmaDirection::ToDevice);
        dma.sync_object_for_device(&obj, DmaDirection::FromDevice);

        self.regmap
            .write(Self::DMA_ACCESS_REG, u64::from(access.dma_addr()).to_be());

        loop {
            dma.sync_object_for_cpu(&access, DmaDirection::ToDevice);

            let ctrl_bits = access.as_ref().control.swap_bytes();
            let ctrl = FwCfgDmaAccessControl::from_bits_truncate(ctrl_bits);

            if ctrl.is_empty() {
                break; // DMA transfer completed successfully
            }
            if ctrl.contains(FwCfgDmaAccessControl::ERROR) {
                return Err(DriverError::UnexpectedError("DMA access failed with error"));
            }
        }

        dma.free(access);

        dma.sync_object_for_cpu(&obj, DmaDirection::FromDevice);

        // SAFETY: by construction, the DMA object is initialized by the device
        let obj = unsafe { obj.assume_init() };
        let v = *obj.as_ref(); // copy the value out of the DMA object
        dma.free(obj);

        Ok(v)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct FwCfgDmaAccess {
    control: u32, // BE - selector (optional) + control bits
    length: u32,  // BE - length of the data to transfer
    address: u64, // BE - physical address of the buffer to transfer
}

impl FwCfgDmaAccess {
    pub fn new(control: FwCfgDmaAccessControl, length: u32, address: u64) -> Self {
        Self {
            control: control.bits().to_be(),
            length: length.to_be(),
            address: address.to_be(),
        }
    }
}

bitflags! {
    struct FwCfgDmaAccessControl: u32 {
        const ERROR = 0x1;
        const READ = 0x2;
        const SKIP = 0x4;
        const SELECT = 0x8;
        const WRITE = 0x10;
    }
}

impl FwCfgDmaAccessControl {
    pub fn select(selector: u16) -> Self {
        FwCfgDmaAccessControl::from_bits_retain((selector as u32) << 16) | Self::SELECT
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct FwCfgFile {
    pub size: u32,     // BE - size of the file in bytes
    pub selector: u16, // BE - selector to use for reading the file
    pub reserved: u16,
    pub name: [u8; 56], // NUL-terminated string
}
