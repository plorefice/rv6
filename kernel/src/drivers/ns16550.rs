use core::fmt::Write;

use lazy_static::lazy_static;
use spin::Mutex;

use crate::{
    config::{self, ns16550::RegSz},
    mm::mmio::{RO, RW},
};

lazy_static! {
    /// Instance of the UART0 serial port on this machine.
    pub static ref UART0: Mutex<Ns16550> = Mutex::new(Ns16550::new(config::ns16550::BASE_ADDRESS));
}

/// Device driver of the 16550 UART IC.
pub struct Ns16550 {
    p: &'static mut RegisterBlock,
}

#[repr(C)]
struct RegisterBlock {
    pub rthr: RW<RegSz>,
    pub ier: RW<RegSz>,
    pub isfcr: RO<RegSz>,
    pub lcr: RW<RegSz>,
    pub mcr: RW<RegSz>,
    pub lsr: RO<RegSz>,
    pub msr: RO<RegSz>,
    pub spr: RW<RegSz>,
}

impl Ns16550 {
    /// Creates a new 16550 UART mapping to the given address.
    pub fn new(addr: usize) -> Self {
        Self {
            p: unsafe { &mut *(addr as *mut RegisterBlock) },
        }
    }

    /// Writes a single byte to the serial interface.
    pub fn put(&mut self, val: u8) {
        while self.p.lsr.read() & 0b0010_0000 == 0 {}
        unsafe { self.p.rthr.write(val as RegSz) };
    }

    /// Returns the next received byte, or `None` if the Rx queue is empty.
    pub fn get(&mut self) -> Option<u8> {
        if self.data_ready() {
            Some((self.p.rthr.read() & 0xff) as u8)
        } else {
            None
        }
    }

    /// Returns true if there is data available in the Rx FIFO.
    pub fn data_ready(&self) -> bool {
        self.p.lsr.read() & 0x1 != 0
    }
}

impl Write for Ns16550 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put('\r' as u8);
            }
            self.put(b);
        }
        Ok(())
    }
}
