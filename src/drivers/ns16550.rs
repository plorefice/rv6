use crate::mmio::{RO, RW};

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
    pub fn new(addr: usize) -> Self {
        Self {
            p: unsafe { &mut *(addr as *mut RegisterBlock) },
        }
    }

    /// Writes a single byte to the serial interface.
    pub fn put(&mut self, val: u8) {
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
