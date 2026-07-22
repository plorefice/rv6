//! Interrupt controller drivers.

use core::{
    iter::{self, FromIterator},
    ops::{Deref, DerefMut},
};

use alloc::{boxed::Box, collections::VecDeque};
use fdt::{Fdt, StringList};
pub use sifive_plic::*;

use crate::{
    drivers::{DriverCtx, DriverError, DynDriverInfo},
    sync::{IrqSpinLock, IrqSpinLockGuard},
};

mod sifive_plic;

/// An interrupt controller.
pub trait InterruptController: Sync + Send {
    /// Sets the priority of the given interrupt.
    fn set_priority(&self, irq: u32, priority: u32);

    /// Sets the threshold for the interrupt controller.
    /// Interrupts with priority less than or equal to the threshold will be masked.
    fn set_threshold(&self, threshold: u32);

    /// Enables the given interrupt line.
    fn enable(&self, irq: u32);

    /// Disables the given interrupt line.
    fn disable(&self, irq: u32);

    /// Claims the next pending interrupt.
    /// Returns the interrupt number if there is a pending interrupt, or `None` if there are
    /// no pending interrupts.
    fn claim(&self) -> Option<u32>;

    /// Completes the handling of the given interrupt.
    fn complete(&self, irq: u32);
}

/// Global IRQ chip, protected by a spinlock for safe concurrent access.
static PLIC: IrqSpinLock<Option<Box<dyn InterruptController>>> = IrqSpinLock::new(None);

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

/// Guard for accessing the global IRQ chip.
pub struct IrqChipGuard<'a> {
    inner: IrqSpinLockGuard<'a, Option<Box<dyn InterruptController>>>,
}

impl Deref for IrqChipGuard<'_> {
    type Target = dyn InterruptController;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("IRQ chip not initialized")
            .deref()
    }
}

impl DerefMut for IrqChipGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("IRQ chip not initialized")
            .deref_mut()
    }
}

/// Returns an exclusive guard to the global IRQ chip.
pub fn global() -> IrqChipGuard<'static> {
    IrqChipGuard { inner: PLIC.lock() }
}
