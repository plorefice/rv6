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
    fb::{DrawTarget, Framebuffer, Pixel, Point, VGA8X16},
    mm::dma::{self, DmaAllocatorExt, DmaDirection, DmaSlice},
};

const FB_WIDTH: usize = 1024;
const FB_HEIGHT: usize = 768;

struct RamFb {
    fbmem: DmaSlice<'static, u32>,
}

impl DrawTarget for RamFb {
    fn width(&self) -> usize {
        FB_WIDTH
    }

    fn height(&self) -> usize {
        FB_HEIGHT
    }

    fn draw_iter<I>(&mut self, pixels: I)
    where
        I: IntoIterator<Item = Pixel>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }

            let x = point.x as usize;
            let y = point.y as usize;

            if x < FB_WIDTH && y < FB_HEIGHT {
                self.fbmem.as_mut_slice()[y * FB_WIDTH + x] = color;
            }
        }

        self.fbmem.sync_for_device(DmaDirection::ToDevice);
    }

    fn clear(&mut self, color: u32) {
        self.fbmem.as_mut_slice().fill(color);
        self.fbmem.sync_for_device(DmaDirection::ToDevice);
    }
}

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

    let mut fb = Framebuffer::new(RamFb { fbmem });
    fb.draw_text(&VGA8X16, Point::ZERO, "Hello, rv6!", 0x00ff00);

    kprintln!(
        "QEMU ramfb: framebuffer at 0x{:x}, size {}x{}, format XR24",
        cfg.addr.to_be(),
        cfg.width.to_be(),
        cfg.height.to_be()
    );

    mem::forget(fb); // Prevent fb from being dropped, will register it later

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
