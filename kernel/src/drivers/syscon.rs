use lazy_static::lazy_static;
use spin::Mutex;

use crate::mm::mmio::RW;

lazy_static! {
    /// Instance of the system controller on this machine.
    /// SAFETY: assuming the configuration is correct and BASE_ADDRESS is valid
    pub static ref SYSCON: Mutex<Syscon> = Mutex::new(unsafe { Syscon::new(0x0010_0000) });
}

/// Device driver of the system controller peripheral.
pub struct Syscon {
    p: &'static mut RegisterBlock,
}

#[repr(C)]
struct RegisterBlock {
    pub reg: RW<u32>,
}

impl Syscon {
    /// Creates a new system controller mapped at the given address.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the address points to a valid instance of the peripheral.
    pub unsafe fn new(addr: usize) -> Self {
        Self {
            // SAFETY: assuming the caller upheld the safety contract
            p: unsafe { &mut *(addr as *mut RegisterBlock) },
        }
    }

    /// Sends a shutdown signal to the system controller with the given exit code.
    pub fn poweroff(&mut self, code: u32) {
        // SAFETY: write with no memory side effects
        unsafe { self.p.reg.write((code << 16) | 0x3333) };
    }

    /// Sends a reboot signal to the system controller.
    pub fn reboot(&mut self) {
        // SAFETY: write with no memory side effects
        unsafe { self.p.reg.write(0x7777) };
    }
}
