use core::fmt::Write;

use lazy_static::lazy_static;
use spin::Mutex;

use crate::mm::mmio::{RO, RW};

lazy_static! {
    /// Instance of the UART0 serial port on this machine.
    pub static ref UART0: Mutex<Ns16550> = Mutex::new(Ns16550::new(0x1000_0000));
}

/// Device driver of the 16550 UART IC.
pub struct Ns16550 {
    p: &'static mut RegisterBlock,
}

#[repr(C, packed)]
struct RegisterBlock {
    pub rthr: RW<u8>,
    pub ier: RW<u8>,
    pub isfcr: RO<u8>,
    pub lcr: RW<u8>,
    pub mcr: RW<u8>,
    pub lsr: RO<u8>,
    pub msr: RO<u8>,
    pub spr: RW<u8>,
}

impl Ns16550 {
    /// Creates a new 16550 UART mapping to the given address.
    pub const fn new(addr: usize) -> Self {
        Self {
            p: unsafe { &mut *(addr as *mut RegisterBlock) },
        }
    }

    /// Writes a single byte to the serial interface.
    pub fn put(&mut self, val: u8) {
        while self.p.lsr.read() & 0b0010_0000 == 0 {}
        unsafe { self.p.rthr.write(val) };
    }

    /// Returns the next received byte, or `None` if the Rx queue is empty.
    pub fn get(&mut self) -> Option<u8> {
        if self.data_ready() {
            Some(self.p.rthr.read())
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
            self.put(b);
        }
        Ok(())
    }
}
