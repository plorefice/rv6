//! Support for 16550 UART IC.

use core::{fmt::Write, hint, num::NonZeroUsize};

use alloc::sync::Arc;
use fdt::Node;
use spin::Mutex;
use uapi::Errno;

use crate::{
    console, driver_info,
    drivers::{Driver, DriverCtx},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        mmio::{self, IoMapper, IoMapping},
    },
    vfs::file_ops::FileOps,
};

use super::DriverError;

driver_info! {
    type: Ns16550,
    of_match: ["ns16550a"],
}

/// Device driver of the 16550 UART IC.
pub struct Ns16550 {
    regmap: IoMapping,
}

impl Driver for Ns16550 {
    fn init<'d, 'fdt: 'd>(_: &DriverCtx, node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>> {
        let (base, size) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let pa_base = PhysAddr::new(base as usize);
        let size =
            NonZeroUsize::new(size as usize).ok_or(DriverError::InvalidPropertyValue("reg"))?;

        let regmap = mmio::mapper().iomap(pa_base, size).unwrap();

        let slf = Self { regmap };

        kprintln!("ns16550: UART at 0x{:x}", base);

        console::register(Arc::new(slf));

        Ok(())
    }
}

impl Ns16550 {
    const RTHR: usize = 0;
    const LSR: usize = 5;

    /// Writes a single byte to the serial interface.
    pub fn put(&self, val: u8) {
        while self.regmap.read::<u8>(Self::LSR) & 0b0010_0000 == 0 {
            hint::spin_loop();
        }
        self.regmap.write(Self::RTHR, val);
    }

    /// Returns the next received byte, or `None` if the Rx queue is empty.
    pub fn get(&self) -> Option<u8> {
        self.data_ready().then(|| self.regmap.read(Self::RTHR))
    }

    /// Returns true if there is data available in the Rx FIFO.
    pub fn data_ready(&self) -> bool {
        self.regmap.read::<u8>(Self::LSR) & 0x1 != 0
    }
}

impl Write for Ns16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put(b'\r');
            }
            self.put(b);
        }
        Ok(())
    }
}

impl FileOps for Ns16550 {
    fn read(&self, _off: &Mutex<u64>, buf: &mut [u8]) -> Result<usize, Errno> {
        let mut i = 0;
        while i < buf.len() {
            if let Some(b) = self.get() {
                // TODO: this should be handled at a higher level,
                // but for now we handle it here to avoid issues with the shell.
                buf[i] = if b == b'\r' { b'\n' } else { b };
                i += 1;
            } else if i == 0 {
                return Err(Errno::Again);
            } else {
                break;
            }
        }
        Ok(i)
    }

    fn write(&self, _off: &Mutex<u64>, buf: &[u8]) -> Result<usize, Errno> {
        for &b in buf {
            self.put(b);
        }
        Ok(buf.len())
    }

    fn seek(&self, _off: &Mutex<u64>, _whence: core::io::SeekFrom) -> Result<u64, Errno> {
        Err(Errno::NoSys)
    }
}
