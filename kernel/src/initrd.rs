//! Initial ramdisk (initrd) support.

use core::fmt;

use fdt::Fdt;

use crate::{
    arch::hal,
    mm::addr::{MemoryAddress, PhysAddr},
};

/// Represents an initial ramdisk.
pub struct Initrd {
    data: &'static [u8],
}

impl Initrd {
    /// Creates a new `Initrd` from the given data.
    pub fn new(data: &'static [u8]) -> Self {
        Initrd { data }
    }

    /// Returns the contents of a file in the initrd by its path.
    pub fn find_file(&self, path: &str) -> Option<&'static [u8]> {
        cpio::find_file(self.data, path).ok().flatten()
    }
}

/// Loads initrd data from the location specified in the FDT.
pub fn load_from_fdt(fdt: &Fdt) -> Result<Initrd, InitrdError> {
    let chosen = fdt
        .find_by_path("/chosen")
        .map_err(|_| InitrdError::NotFound)?;

    let start = chosen
        .as_ref()
        .and_then(|node| {
            node.property::<(u32, u32)>("linux,initrd-start")
                .map(|(hi, lo)| PhysAddr::new(((hi as usize) << 32) | (lo as usize)))
        })
        .ok_or(InitrdError::NotFound)?;

    let end = chosen
        .and_then(|node| {
            node.property::<(u32, u32)>("linux,initrd-end")
                .map(|(hi, lo)| PhysAddr::new(((hi as usize) << 32) | (lo as usize)))
        })
        .ok_or(InitrdError::NotFound)?;

    let len = (end - start).as_usize();

    kprintln!(
        "Initrd found in FDT: start=0x{:x}, end=0x{:x}, size={} bytes",
        start,
        end,
        len
    );

    // SAFETY: we trust the FDT to provide us with valid physical addresses for the initrd
    let initrd_data = unsafe {
        let ptr = hal::mm::phys_to_virt(start).as_ptr::<u8>();
        core::slice::from_raw_parts(ptr, len)
    };

    Ok(Initrd::new(initrd_data))
}

/// Errors related to initrd.
#[derive(Debug)]
pub enum InitrdError {
    /// The initrd was not found in the FDT.
    NotFound,
}

impl fmt::Display for InitrdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitrdError::NotFound => write!(f, "initrd not reference in FDT"),
        }
    }
}
