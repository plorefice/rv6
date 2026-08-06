//! Support for 16550 UART IC.

use core::{hint, num::NonZeroUsize};

use alloc::sync::Arc;
use fdt::Node;

use crate::{
    console, driver_info,
    drivers::{Driver, DriverCtx},
    irq::{self, IrqReturn},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        mmio::{self, IoMapper, IoMapping},
    },
    tty::{Tty, TtyDevice},
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

        let uart = Arc::new(Self { regmap });
        let tty = Arc::new(Tty::new(uart.clone()));

        // Configure the interrupt controller to enable interrupts for the UART
        let irqn = node
            .property::<u32>("interrupts")
            .ok_or(DriverError::MissingRequiredProperty("interrupts"))?;

        let (irq_uart, irq_tty) = (uart.clone(), tty.clone());
        irq::request_irq(
            irqn,
            Arc::new(move || {
                while let Some(b) = irq_uart.get_raw() {
                    irq_tty.receive_byte(b);
                }
                IrqReturn::Handled
            }),
        );
        uart.enable_interrupts();

        kprintln!("ns16550: UART at 0x{:x}", base);

        console::register(tty);

        Ok(())
    }
}

impl Ns16550 {
    const RTHR: usize = 0;
    const IER: usize = 1;
    const LSR: usize = 5;

    /// Writes a single byte to the serial interface.
    pub fn put_raw(&self, val: u8) {
        while self.regmap.read::<u8>(Self::LSR) & 0b0010_0000 == 0 {
            hint::spin_loop();
        }
        self.regmap.write(Self::RTHR, val);
    }

    /// Returns the next received byte, or `None` if the Rx queue is empty.
    #[inline]
    pub fn get_raw(&self) -> Option<u8> {
        self.data_ready().then(|| self.regmap.read(Self::RTHR))
    }

    /// Returns true if there is data available in the Rx FIFO.
    #[inline]
    pub fn data_ready(&self) -> bool {
        self.regmap.read::<u8>(Self::LSR) & 0x1 != 0
    }

    /// Enables data ready interrupts for the UART.
    #[inline]
    pub fn enable_interrupts(&self) {
        self.regmap.write::<u8>(Self::IER, 1);
    }

    /// Disables data ready interrupts for the UART.
    #[inline]
    pub fn disable_interrupts(&self) {
        self.regmap.write::<u8>(Self::IER, 0);
    }
}

impl TtyDevice for Ns16550 {
    fn put(&self, byte: u8) {
        self.put_raw(byte);
    }
}
