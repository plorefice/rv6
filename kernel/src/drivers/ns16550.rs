//! Support for 16550 UART IC.

use core::{fmt::Write, hint, num::NonZeroUsize};

use alloc::{collections::VecDeque, sync::Arc};
use fdt::Node;
use uapi::Errno;

use crate::{
    console, driver_info,
    drivers::{Driver, DriverCtx},
    irq::{self, IrqHandler, IrqReturn},
    mm::{
        addr::{MemoryAddress, PhysAddr},
        mmio::{self, IoMapper, IoMapping},
    },
    sync::{SpinLock, WaitQueue},
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
    rx_data: WaitQueue<VecDeque<u8>>,
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

        let slf = Arc::new(Self {
            regmap,
            rx_data: WaitQueue::new(VecDeque::new()),
        });

        // Configure the interrupt controller to enable interrupts for the UART
        let irqn = node
            .property::<u32>("interrupts")
            .ok_or(DriverError::MissingRequiredProperty("interrupts"))?;

        irq::request_irq(irqn, slf.clone());
        slf.enable_interrupts();

        kprintln!("ns16550: UART at 0x{:x}", base);

        console::register(slf);

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

impl IrqHandler for Ns16550 {
    fn handle(&self) -> IrqReturn {
        let mut g = self.rx_data.lock();
        while let Some(b) = self.get_raw() {
            g.push_back(b);
        }
        g.wake_all();

        IrqReturn::Handled
    }
}

impl Write for Ns16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put_raw(b'\r');
            }
            self.put_raw(b);
        }
        Ok(())
    }
}

impl FileOps for Ns16550 {
    fn read(&self, _off: &SpinLock<u64>, buf: &mut [u8]) -> Result<usize, Errno> {
        let mut i = 0;
        loop {
            let mut g = self.rx_data.wait_until(|q| !q.is_empty());

            while let Some(b) = g.pop_front() {
                // TODO: this should be handled at a higher level,
                // but for now we handle it here to avoid issues with the shell.
                buf[i] = if b == b'\r' { b'\n' } else { b };

                i += 1;
                if i >= buf.len() {
                    return Ok(i);
                }
            }

            // At least one byte was read, return it. Otherwise, wait for more data.
            if i > 0 {
                return Ok(i);
            }
        }
    }

    fn write(&self, _off: &SpinLock<u64>, buf: &[u8]) -> Result<usize, Errno> {
        for &b in buf {
            self.put_raw(b);
        }
        Ok(buf.len())
    }

    fn seek(&self, _off: &SpinLock<u64>, _whence: core::io::SeekFrom) -> Result<u64, Errno> {
        Err(Errno::NoSys)
    }
}
