//! QEMU RAM Framebuffer (ramfb) Device Driver
//!
//! ramfb is a simple way to get graphics on QEMU via a framebuffer in memory on embedded platforms
//! like ARM or RISC-V. It works by adding -device ramfb to the QEMU command line and then
//! configuring ramfb via fw_cfg. ramfb requires QEMU DMA support which should be available on
//! platforms that have fw_cfg as MMIO (as opposed to x86 IO ports).

use alloc::{boxed::Box, sync::Arc};

use crate::{
    drivers::{
        DriverError,
        qemu::{FwCfgFile, QemuFwCfg},
    },
    fb::{self, DrawTarget, FbInfo, Point, Rect},
    mm::dma::{self, DmaAllocatorExt, DmaDirection, DmaSlice},
};

const FB_WIDTH: usize = 1024;
const FB_HEIGHT: usize = 768;

struct RamFb {
    fbmem: DmaSlice<'static, u32>,
}

impl DrawTarget for RamFb {
    fn info(&self) -> FbInfo {
        FbInfo {
            width: FB_WIDTH,
            height: FB_HEIGHT,
            stride: FB_WIDTH,
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: u32) {
        let info = self.info();
        let Some(rect) = rect.intersect(info.rect()) else {
            return;
        };

        let left = rect.left() as usize;
        let width = rect.width() as usize;
        let fbmem = self.fbmem.as_mut_slice();

        for y in rect.top() as usize..rect.bottom() as usize {
            let start = y * info.stride + left;
            fbmem[start..start + width].fill(color);
        }
    }

    fn blit(&mut self, rect: Rect, src: &[u32]) {
        debug_assert_eq!(
            src.len(),
            rect.width().max(0) as usize * rect.height().max(0) as usize,
            "blit source must hold exactly one pixel per pixel of `rect`"
        );

        let info = self.info();
        let Some(clipped) = rect.intersect(info.rect()) else {
            return;
        };

        // Offset of the clipped region within the (unclipped) source buffer.
        let src_pitch = rect.width() as usize;
        let src_x = (clipped.left() - rect.left()) as usize;
        let src_y = (clipped.top() - rect.top()) as usize;

        let dst_x = clipped.left() as usize;
        let dst_y = clipped.top() as usize;
        let width = clipped.width() as usize;
        let fbmem = self.fbmem.as_mut_slice();

        for row in 0..clipped.height() as usize {
            let s = (src_y + row) * src_pitch + src_x;
            let d = (dst_y + row) * info.stride + dst_x;
            fbmem[d..d + width].copy_from_slice(&src[s..s + width]);
        }
    }

    fn copy_rect(&mut self, src: Rect, dst: Point) {
        let info = self.info();
        let bounds = info.rect();
        let delta = Point::new(dst.x - src.left(), dst.y - src.top());

        // Keep only the part of `src` that is on-screen *and* whose destination is on-screen.
        // The latter is expressed in source coordinates by shifting the bounds back by `delta`.
        let Some(src) = src
            .intersect(bounds)
            .and_then(|r| r.intersect(bounds.translate(Point::new(-delta.x, -delta.y))))
        else {
            return;
        };

        let src_x = src.left() as usize;
        let src_y = src.top() as usize;
        let dst_x = (src.left() + delta.x) as usize;
        let dst_y = (src.top() + delta.y) as usize;
        let width = src.width() as usize;
        let height = src.height() as usize;

        let stride = info.stride;
        let fbmem = self.fbmem.as_mut_slice();

        // `copy_within` is a memmove, so overlap within a row is safe. Rows, however, must be
        // walked away from the destination so we never overwrite a row before reading it.
        let mut copy_row = |row: usize| {
            let s = (src_y + row) * stride + src_x;
            let d = (dst_y + row) * stride + dst_x;
            fbmem.copy_within(s..s + width, d);
        };

        if dst_y > src_y {
            (0..height).rev().for_each(&mut copy_row);
        } else {
            (0..height).for_each(&mut copy_row);
        }
    }

    fn flush(&mut self, _damage: Rect) {
        // `DmaSlice` can only be synchronized as a whole, so the damage rectangle is ignored.
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

    kprintln!(
        "QEMU ramfb: framebuffer at 0x{:x}, size {}x{}, format XR24",
        cfg.addr.to_be(),
        cfg.width.to_be(),
        cfg.height.to_be()
    );

    // Register the framebuffer with the kernel's graphics subsystem
    fb::register(Box::new(RamFb { fbmem }));

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
