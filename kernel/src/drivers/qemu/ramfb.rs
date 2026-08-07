//! QEMU RAM Framebuffer (ramfb) Device Driver
//!
//! ramfb is a simple way to get graphics on QEMU via a framebuffer in memory on embedded platforms
//! like ARM or RISC-V. It works by adding -device ramfb to the QEMU command line and then
//! configuring ramfb via fw_cfg. ramfb requires QEMU DMA support which should be available on
//! platforms that have fw_cfg as MMIO (as opposed to x86 IO ports).

use core::mem;

use alloc::sync::Arc;

use crate::{
    drivers::{
        DriverError,
        qemu::{FwCfgFile, QemuFwCfg},
    },
    mm::dma::{self, DmaAllocatorExt},
};

const FB_WIDTH: usize = 1024;
const FB_HEIGHT: usize = 768;

/// Probes the QEMU ramfb device and initializes it.
///
/// This function allocates a framebuffer in DMA memory, configures the ramfb device via fw_cfg
/// and registers the framebuffer with the kernel's graphics subsystem.
pub fn probe(fw_cfg: Arc<QemuFwCfg>, ramfb_file: FwCfgFile) -> Result<(), DriverError<'static>> {
    let fbmem = dma::allocator().alloc_slice_zeroed::<u32>(FB_WIDTH * FB_HEIGHT)?;

    let cfg = RamFbCfg::new(
        fbmem.dma_addr().into(),
        u32::from_ne_bytes(*b"XR24"),
        FB_WIDTH as u32,
        FB_HEIGHT as u32,
    );

    fw_cfg.write(dma::allocator(), Some(ramfb_file.selector), cfg)?;

    mem::forget(fbmem); // Prevent fbmem from being dropped, as it is now managed by the ramfb device

    kprintln!(
        "QEMU ramfb: framebuffer at 0x{:x}, size {}x{}, format XR24",
        cfg.addr.to_be(),
        cfg.width.to_be(),
        cfg.height.to_be()
    );

    Ok(())
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct RamFbCfg {
    pub addr: u64,   // BE - physical address of the framebuffer
    pub fourcc: u32, // BE - fourcc pixel format code
    pub flags: u32,  // BE - flags (reserved, must be 0)
    pub width: u32,  // BE - width of the framebuffer in pixels
    pub height: u32, // BE - height of the framebuffer in pixels
    pub stride: u32, // BE - if 0, the stride is width * bpp
}

impl RamFbCfg {
    pub fn new(addr: u64, fourcc: u32, width: u32, height: u32) -> Self {
        Self {
            addr: addr.to_be(),
            fourcc: fourcc.to_be(),
            flags: 0,
            width: width.to_be(),
            height: height.to_be(),
            stride: 0,
        }
    }
}
