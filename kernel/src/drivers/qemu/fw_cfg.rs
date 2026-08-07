//! QEMU Firmware Configuration (fw_cfg) Device Driver
//!
//! This driver provides an interface to the QEMU Firmware Configuration (fw_cfg) device, which
//! allows the guest operating system to access configuration data provided by the QEMU hypervisor.
//! The fw_cfg device is typically used to retrieve information about the virtual machine's
//! configuration, such as the presence of specific files or features.

use core::{mem, num::NonZeroUsize};

use alloc::sync::Arc;
use bitflags::bitflags;
use fdt::Node;

use crate::{
    driver_info,
    drivers::{Driver, DriverCtx, DriverError, qemu::ramfb},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        dma::{self, DmaAllocator, DmaAllocatorExt, DmaDirection, DmaObject, DmaSafe},
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

        let fw_cfg = Arc::new(Self { regmap });

        if fw_cfg.signature()? != *b"QEMU" {
            return Err(DriverError::UnexpectedError("Invalid signature"));
        }
        if fw_cfg.features()? & 0x2 == 0 {
            return Err(DriverError::UnexpectedError("DMA not supported"));
        }

        kprintln!("QEMU FW_CFG: configured");

        if let Some(ramfb_file) = fw_cfg.find_file("etc/ramfb")? {
            ramfb::probe(fw_cfg, ramfb_file)?;
        }

        Ok(())
    }
}

impl QemuFwCfg {
    const DMA_ACCESS_REG: usize = 0x10;

    const FW_CFG_SIGNATURE: u16 = 0x0000;
    const FW_CFG_ID: u16 = 0x0001;
    const FW_CFG_FILE_DIR: u16 = 0x0019;

    fn signature(&self) -> Result<[u8; 4], DriverError<'static>> {
        self.read(dma::allocator(), Some(Self::FW_CFG_SIGNATURE))
    }

    fn features(&self) -> Result<u32, DriverError<'static>> {
        self.read(dma::allocator(), Some(Self::FW_CFG_ID))
    }

    fn find_file(&self, name: &str) -> Result<Option<FwCfgFile>, DriverError<'static>> {
        let dma = dma::allocator();
        let count = u32::from_be(self.read(dma, Some(Self::FW_CFG_FILE_DIR))?);

        for _ in 0..count {
            let mut file = self.read::<FwCfgFile>(dma, None)?; // read the next file entry

            let file_name = match core::str::from_utf8(&file.name) {
                Ok(s) => s.trim_end_matches('\0'),
                Err(_) => continue,
            };

            if file_name == name {
                // Fix endianness of the fields in the file entry
                file.size = u32::from_be(file.size);
                file.selector = u16::from_be(file.selector);

                return Ok(Some(file));
            }
        }

        Ok(None)
    }

    /// Reads a value of type `T` at the specified selector from the fw_cfg device using DMA.
    pub fn read<T: DmaSafe>(
        &self,
        dma: &dyn DmaAllocator,
        selector: Option<u16>,
    ) -> Result<T, DriverError<'static>> {
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

        self.wait_for_dma_completion(dma, &access)?;

        dma.sync_object_for_cpu(&obj, DmaDirection::FromDevice);

        // SAFETY: by construction, the DMA object is initialized by the device
        let obj = unsafe { obj.assume_init() };
        Ok(*obj.as_ref())
    }

    /// Writes a value at the specified selector to the fw_cfg device using DMA.
    pub fn write<T: DmaSafe>(
        &self,
        dma: &dyn DmaAllocator,
        selector: Option<u16>,
        val: T,
    ) -> Result<(), DriverError<'static>> {
        let obj = dma.alloc(val)?;

        let access = dma.alloc(FwCfgDmaAccess::new(
            selector
                .map(FwCfgDmaAccessControl::select)
                .unwrap_or(FwCfgDmaAccessControl::empty())
                | FwCfgDmaAccessControl::WRITE,
            mem::size_of::<T>() as u32,
            obj.dma_addr().into(),
        ))?;

        dma.sync_object_for_device(&access, DmaDirection::ToDevice);
        dma.sync_object_for_device(&obj, DmaDirection::ToDevice);

        self.regmap
            .write(Self::DMA_ACCESS_REG, u64::from(access.dma_addr()).to_be());

        self.wait_for_dma_completion(dma, &access)?;

        Ok(())
    }

    fn wait_for_dma_completion(
        &self,
        dma: &dyn DmaAllocator,
        access: &DmaObject<'_, FwCfgDmaAccess>,
    ) -> Result<(), DriverError<'static>> {
        loop {
            dma.sync_object_for_cpu(access, DmaDirection::ToDevice);

            let ctrl_bits = u32::from_be(access.as_ref().control);
            let ctrl = FwCfgDmaAccessControl::from_bits_truncate(ctrl_bits);

            if ctrl.is_empty() {
                break; // DMA transfer completed successfully
            }
            if ctrl.contains(FwCfgDmaAccessControl::ERROR) {
                return Err(DriverError::UnexpectedError("DMA access failed with error"));
            }
        }

        Ok(())
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

/// Represents a file entry in the QEMU fw_cfg file directory.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FwCfgFile {
    /// The size of the file in bytes.
    pub size: u32,
    /// The selector to use for reading the file
    pub selector: u16,
    reserved: u16,
    /// The name of the file, as a NUL-terminated string.
    pub name: [u8; 56],
}
