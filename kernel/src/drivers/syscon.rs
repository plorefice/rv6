//! System controller registers for poweroff and reboot.

use fdt::{Node, PropEncodedArray};

use crate::{arch::PhysAddr, drivers::DriverError, mm::mmio::Regmap};

pub(crate) struct SysconDriverInfo;

impl super::DriverInfo for SysconDriverInfo {
    type Driver = Syscon;

    fn of_match() -> &'static [&'static str] {
        &["syscon"]
    }
}

/// System controller.
pub struct Syscon {
    regmap: Regmap,
}

impl super::Driver for Syscon {
    fn init<'d>(fdt: Node<'d, '_>) -> Result<Self, DriverError<'d>> {
        let mut regs = fdt
            .property::<PropEncodedArray<(u64, u64)>>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let (base, len) = regs.next().expect("empty reg property");

        Ok(Self {
            // SAFETY: assuming the node contains a valid regmap
            regmap: unsafe { Regmap::new(PhysAddr::new(base), len) },
        })
    }
}

impl Syscon {
    /// Sends a shutdown signal to the system controller with the given exit code.
    pub fn poweroff(&self, code: u32) {
        self.regmap.write::<u32>(0, (code << 16) | 0x3333);
    }

    /// Sends a reboot signal to the system controller.
    pub fn reboot(&self) {
        self.regmap.write::<u32>(0, 0x7777);
    }
}
