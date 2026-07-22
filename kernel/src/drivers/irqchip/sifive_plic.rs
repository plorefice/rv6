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
        mmio::{self, IoMapper, IoMapping},
    },
};

const PLIC_CONTEXT: usize = 1; // hart 0, supervisor mode

driver_info! {
    type: SifivePlic,
    of_match: ["riscv,plic0"],
}

/// SiFive Platform-Level Interrupt Controller (PLIC).
pub struct SifivePlic {
    regmap: IoMapping,
}

impl Driver for SifivePlic {
    fn init<'d, 'fdt: 'd>(_: &DriverCtx, node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>>
    where
        Self: Sized,
    {
        let (base, len) = node
            .property::<(u64, u64)>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let pa_base = PhysAddr::new(base as usize);
        let size =
            NonZeroUsize::new(len as usize).ok_or(DriverError::InvalidPropertyValue("reg"))?;

        let regmap = mmio::mapper().iomap(pa_base, size).unwrap();

        kprintln!("PLIC: {:#x} - {:#x}", base, base + len);

        let slf = Self { regmap };

        // By default, allow all interrupts with priority > 0
        slf.set_threshold(0);

        irqchip::register_platform_irqchip(slf);

        Ok(())
    }
}

impl InterruptController for SifivePlic {
    fn set_priority(&self, irq: u32, priority: u32) {
        self.regmap.write((4 * irq) as usize, priority);
    }

    fn set_threshold(&self, threshold: u32) {
        self.regmap
            .write(0x20_0000 + 0x1000 * PLIC_CONTEXT, threshold);
    }

    fn enable(&self, irq: u32) {
        let offset = 0x2000 + 0x80 * PLIC_CONTEXT + 4 * (irq / 32) as usize;
        let mask = 1 << (irq % 32);
        let val = self.regmap.read::<u32>(offset);
        self.regmap.write(offset, val | mask);
    }

    fn disable(&self, irq: u32) {
        let offset = 0x2000 + 0x80 * PLIC_CONTEXT + 4 * (irq / 32) as usize;
        let mask = !(1 << (irq % 32));
        let val = self.regmap.read::<u32>(offset);
        self.regmap.write(offset, val & mask);
    }

    fn claim(&self) -> Option<u32> {
        let offset = 0x20_0000 + 0x1000 * PLIC_CONTEXT + 4;
        let irq = self.regmap.read::<u32>(offset);
        if irq == 0 { None } else { Some(irq) }
    }

    fn complete(&self, irq: u32) {
        let offset = 0x20_0000 + 0x1000 * PLIC_CONTEXT + 4;
        self.regmap.write(offset, irq);
    }
}
