//! Interrupt controller drivers.

use core::iter::{self, FromIterator};

use alloc::{boxed::Box, collections::VecDeque};
use fdt::{Fdt, StringList};
pub use sifive_plic::*;
use spin::Mutex;

use crate::drivers::{DriverCtx, DriverError, DynDriverInfo};

mod sifive_plic;

/// An interrupt controller.
pub trait InterruptController: Sync + Send {}

/// Global IRQ chip.
static PLIC: Mutex<Option<Box<dyn InterruptController>>> = Mutex::new(None);

/// Initializes the platform IRQ chip(s).
pub fn init<'d>(ctx: &DriverCtx, fdt: &'d Fdt<'d>) -> Result<(), DriverError<'d>> {
    // TODO: global vector with dynamic registration maybe?
    let infos: &[&dyn DynDriverInfo] = &[&SifivePlicDriverInfo];

    let mut nodes = VecDeque::from_iter(iter::once(fdt.root_node()?));

    while let Some(node) = nodes.pop_front() {
        nodes.extend(node.children());

        let Some(compatibles) = node.property::<StringList>("compatible") else {
            continue;
        };

        if let Some(modinfo) = infos
            .iter()
            .find(|i| compatibles.clone().any(|c| i.of_match().contains(&c)))
            && let Err(e) = modinfo.init(ctx, node)
        {
            kprintln!("Error: Failed to init IRQ chip: {:?}", e);
        };
    }

    Ok(())
}

fn register_platform_irqchip<T>(irqchip: T)
where
    T: InterruptController + 'static,
{
    let mut plic = PLIC.lock();

    if plic.is_none() {
        *plic = Some(Box::new(irqchip));
    } else {
        kprintln!("Error: only one platform irqchip is supported!")
    }
}
