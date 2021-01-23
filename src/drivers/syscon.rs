use lazy_static::lazy_static;
use spin::Mutex;

use crate::mm::mmio::RW;

lazy_static! {
    /// Instance of the UART0 serial port on this machine.
    pub static ref SYSCON: Mutex<Syscon> = Mutex::new(Syscon::new(0x0010_0000));
}

/// Device driver of the SYSCON peripheral.
pub struct Syscon {
    p: &'static mut RegisterBlock,
}

#[repr(C, packed)]
struct RegisterBlock {
    pub reg: RW<u32>,
}

impl Syscon {
    /// Creates a new SYSCON mapping at the given address.
    pub const fn new(addr: usize) -> Self {
        Self {
            p: unsafe { &mut *(addr as *mut RegisterBlock) },
        }
    }

    /// Sends a shutdown signal to the SYSCON with the given exit code.
    pub fn poweroff(&mut self, code: u32) {
        unsafe { self.p.reg.write((code << 16) | 0x5555) };
    }

    /// Sends a reboot signal to the SYSCON.
    pub fn reboot(&mut self) {
        unsafe { self.p.reg.write(0x7777) };
    }
}
