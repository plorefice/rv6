use fdt::Node;

use crate::{
    driver_info,
    drivers::{
        irqchip::{self, InterruptController},
        Driver, DriverError,
    },
    mm::mmio::Regmap,
};

driver_info! {
    type: SifivePlic,
    of_match: ["riscv,plic0"],
}

/// SiFive Platform-Level Interrupt Controller (PLIC).
pub struct SifivePlic {
    _regmap: Regmap,
}

impl Driver for SifivePlic {
    fn init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>>
    where
        Self: Sized,
    {
        let (base, len) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        // SAFETY: assuming the node contains a valid regmap
        let regmap = unsafe { Regmap::new(base, len) };

        kprintln!("PLIC: {:#x} - {:#x}", base, base + len);

        irqchip::register_platform_irqchip(SifivePlic { _regmap: regmap });

        Ok(())
    }
}

impl InterruptController for SifivePlic {}
