//! Support for 16550 UART IC.

use core::{fmt::Write, hint, num::NonZeroUsize};

use fdt::Node;

use crate::{
    driver_info,
    drivers::{Driver, DriverCtx},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        mmio::{IoMapper, IoMapping},
    },
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
    fn init<'d, 'fdt: 'd>(ctx: &DriverCtx, node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>> {
        let (base, size) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let pa_base = PhysAddr::new(base as usize);
        let size =
            NonZeroUsize::new(size as usize).ok_or(DriverError::InvalidPropertyValue("reg"))?;

        let regmap = ctx.arch.io.iomap(pa_base, size).unwrap();

        let mut slf = Self { regmap };

        kprintln!("ns16550: UART at 0x{:x}", base);
        writeln!(slf, "*** Hello, world! ***").ok();

        // TODO: register this as console device

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
