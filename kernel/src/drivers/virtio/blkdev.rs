use core::{ffi::CStr, hint};

use alloc::string::String;
use bitflags::bitflags;

use crate::{
    arch::palloc,
    drivers::virtio::{
        virtq::{Virtq, VirtqBuffer},
        InterruptStatus, Status, VirtioDev, VirtioDriver,
    },
    mm::PhysicalAddress,
};

/// A virtio block device.
pub struct VirtioBlkDev<D> {
    dev: D,
    virtq: Virtq,
    config: VirtioBlkConfig,
    features: DeviceFeatures,
}

impl<D: VirtioDev> VirtioBlkDev<D> {
    /// Configures a virtio device as block device.
    pub fn new(dev: D) -> Self {
        // Recognize the device
        dev.update_status(Status::ACKNOWLEDGE);
        dev.update_status(Status::DRIVER);

        // Read and acknowledge device features
        let features = DeviceFeatures::from_bits_retain(dev.read_device_features(0))
            & (DeviceFeatures::SIZE_MAX
                | DeviceFeatures::SEG_MAX
                | DeviceFeatures::GEOMETRY
                | DeviceFeatures::BLK_SIZE
                | DeviceFeatures::TOPOLOGY);
        dev.enable_device_features(0, features.bits());

        // Configure virtqueues
        let virtq = dev.allocate_virtq(0);

        let mut slf = Self {
            dev,
            virtq,
            features,
            config: VirtioBlkConfig::default(),
        };

        // Read configuration
        slf.read_config();
        kprintln!(
            "virtio-blk-dev: {} sectors disk ({} MiB), {}c/{}h/{}s",
            slf.config.capacity,
            (slf.config.capacity * 512) / (1 << 20),
            slf.config.geometry.cylinders,
            slf.config.geometry.heads,
            slf.config.geometry.sectors
        );

        // Device is now live
        slf.dev.update_status(Status::DRIVER_OK);

        // Check device ID if available
        if let Some(id) = slf.read_device_id() {
            kprintln!("virtio-blk-dev: device ID: {id}");
        }

        slf
    }

    fn read_config(&mut self) {
        let cfg = &mut self.config;

        cfg.capacity = self.dev.read_config(0);

        if self.features.contains(DeviceFeatures::SIZE_MAX) {
            cfg.size_max = self.dev.read_config(8);
        }

        if self.features.contains(DeviceFeatures::SEG_MAX) {
            cfg.seg_max = self.dev.read_config(12);
        }

        if self.features.contains(DeviceFeatures::GEOMETRY) {
            cfg.geometry = VirtioBlkGeometry {
                cylinders: self.dev.read_config(16),
                heads: self.dev.read_config(18),
                sectors: self.dev.read_config(19),
            };
        }

        if self.features.contains(DeviceFeatures::BLK_SIZE) {
            cfg.blk_size = self.dev.read_config(20);
        }

        if self.features.contains(DeviceFeatures::TOPOLOGY) {
            cfg.topology = VirtioBlkTopology {
                physical_block_exp: self.dev.read_config(24),
                alignment_offset: self.dev.read_config(25),
                min_io_size: self.dev.read_config(26),
                opt_io_size: self.dev.read_config(28),
            };
        }
    }

    fn read_device_id(&mut self) -> Option<String> {
        let buf = palloc([0_u8; 20]);

        self.transfer(VirtioBlkReqType::GetId, 0, buf.phys_addr(), 20);

        let id = CStr::from_bytes_until_nul(buf.as_ref()).ok()?;
        if id.is_empty() {
            return None;
        }

        id.to_str().ok().map(String::from)
    }

    fn transfer(
        &mut self,
        kind: VirtioBlkReqType,
        sector: u64,
        data: impl PhysicalAddress<u64>,
        len: usize,
    ) {
        use VirtioBlkReqType::*;
        use VirtqBuffer::*;

        let blk_req = palloc(VirtioBlkReq {
            kind,
            rsvd: 0,
            sector,
            status: 0,
        });

        // TODO: validate data size according to kinds
        let data_buf = match kind {
            In | GetId => Some(Writeable {
                addr: data.into(),
                len,
            }),
            Flush => None,
            Out | GetLifetime | Discard | WriteZeroes | SecureErase => Some(Readable {
                addr: data.into(),
                len,
            }),
        };

        self.virtq.submit(
            &self.dev,
            [
                Some(VirtqBuffer::Readable {
                    addr: blk_req.phys_addr().into(),
                    len: VirtioBlkReq::HEADER_SIZE,
                }),
                data_buf,
                Some(VirtqBuffer::Writeable {
                    addr: blk_req.phys_addr().into() + VirtioBlkReq::HEADER_SIZE as u64,
                    len: VirtioBlkReq::TRAILER_SIZE,
                }),
            ]
            .iter()
            .filter_map(Option::as_ref),
        );

        // TODO: replace this with proper interrupt handling
        while !self.dev.interrupts().contains(InterruptStatus::USED_BUFFER) {
            hint::spin_loop();
        }

        self.dev.clear_interrupts(InterruptStatus::USED_BUFFER);
    }
}

impl<D> VirtioDriver for VirtioBlkDev<D> {}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct VirtioBlkReq {
    kind: VirtioBlkReqType,
    rsvd: u32,
    sector: u64,
    // `data` field intentionally missing
    status: u8,
}

impl VirtioBlkReq {
    const HEADER_SIZE: usize = 16;
    const TRAILER_SIZE: usize = 1;
}

/// Supported request types for a VirtIO block device.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioBlkReqType {
    /// Read sectors from the device
    In = 0,
    /// Write sectors to the device
    Out = 1,
    /// Flush pending writes to the device
    Flush = 4,
    /// Queries the device's ID
    GetId = 8,
    /// Queries the device's lifetime information
    GetLifetime = 10,
    /// ???
    Discard = 11,
    /// Write zeroes to the device
    WriteZeroes = 13,
    /// Securely erases the contents of this device
    SecureErase = 14,
}

#[allow(unused)]
#[derive(Default, Debug, Clone, Copy)]
struct VirtioBlkConfig {
    capacity: u64,
    size_max: u32,
    seg_max: u32,
    geometry: VirtioBlkGeometry,
    blk_size: u32,
    topology: VirtioBlkTopology,
}

#[allow(unused)]
#[derive(Default, Debug, Clone, Copy)]
struct VirtioBlkGeometry {
    cylinders: u16,
    heads: u8,
    sectors: u8,
}

#[allow(unused)]
#[derive(Default, Debug, Clone, Copy)]
struct VirtioBlkTopology {
    physical_block_exp: u8,
    alignment_offset: u8,
    min_io_size: u16,
    opt_io_size: u32,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct DeviceFeatures: u32 {
        const BARRIER      = 1 << 0;
        const SIZE_MAX     = 1 << 1;
        const SEG_MAX      = 1 << 2;
        const GEOMETRY     = 1 << 4;
        const RO           = 1 << 5;
        const BLK_SIZE     = 1 << 6;
        const SCSI         = 1 << 7;
        const FLUSH        = 1 << 9;
        const TOPOLOGY     = 1 << 10;
        const CONFIG_WCE   = 1 << 11;
        const MQ           = 1 << 12;
        const DISCARD      = 1 << 13;
        const WRITE_ZEROES = 1 << 14;
        const LIFETIME     = 1 << 15;
        const SECURE_ERASE = 1 << 16;

        // Device-independent features
        const NOTIFY_ON_EMPTY    = 1 << 24;
        const RING_INDIRECT_DESC = 1 << 28;
        const RING_EVENT_IDX     = 1 << 29;
    }
}
