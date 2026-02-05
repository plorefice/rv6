use core::num::NonZeroUsize;

use fdt::Node;

use crate::{
    driver_info,
    drivers::{
        Driver, DriverCtx, DriverError,
        irqchip::{self, InterruptController},
    },
    mm::{
        addr::{MemoryAddress, PhysAddr},
        mmio::{IoMapper, IoMapping},
    },
};

driver_info! {
    type: SifivePlic,
    of_match: ["riscv,plic0"],
}

/// SiFive Platform-Level Interrupt Controller (PLIC).
pub struct SifivePlic {
    _regmap: IoMapping,
}

impl Driver for SifivePlic {
    fn init<'d, 'fdt: 'd>(ctx: &DriverCtx, node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>>
    where
        Self: Sized,
    {
        let (base, len) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let pa_base = PhysAddr::new(base as usize);
        let size =
            NonZeroUsize::new(len as usize).ok_or(DriverError::InvalidPropertyValue("reg"))?;

        let regmap = ctx.arch.io.iomap(pa_base, size).unwrap();

        kprintln!("PLIC: {:#x} - {:#x}", base, base + len);

        irqchip::register_platform_irqchip(SifivePlic { _regmap: regmap });

        Ok(())
    }
}

impl InterruptController for SifivePlic {}
