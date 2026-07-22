//! Kernel interrupt handling and management.

use alloc::sync::Arc;
use spin::Mutex;

use crate::drivers::irqchip;

/// Callbacks for handling interrupts.
pub trait IrqHandler: Send + Sync + 'static {
    /// Handle an interrupt.
    ///
    /// This is executed in the context of the interrupt handler, so it should be fast and not block.
    fn handle(&self) -> IrqReturn;
}

/// Return value for interrupt handlers.
pub enum IrqReturn {
    /// The interrupt was handled successfully.
    Handled,
    /// The interrupt was not handled and should be passed to the next handler.
    Unhandled,
}

/// Blanket implementation of `IrqHandler` for any function pointer that matches the signature.
impl<F> IrqHandler for F
where
    F: Fn() -> IrqReturn + Send + Sync + 'static,
{
    fn handle(&self) -> IrqReturn {
        self()
    }
}

/// Blanket implementation of `IrqHandler` for any `Arc<dyn IrqHandler>`.
impl IrqHandler for Arc<dyn IrqHandler> {
    fn handle(&self) -> IrqReturn {
        (**self).handle()
    }
}

/// Global interrupt handler registry.
static HANDLERS: Mutex<[Option<Arc<dyn IrqHandler>>; 1024]> = Mutex::new([const { None }; 1024]);

/// Register an interrupt handler for a given IRQ number and enable the interrupt in the global IRQ chip.
///
/// # Panics
///
/// This function will panic if the IRQ number is out of bounds.
pub fn request_irq(irq: u32, handler: Arc<dyn IrqHandler>) {
    HANDLERS.lock()[irq as usize] = Some(handler);

    let ic = irqchip::global();
    ic.set_priority(irq, 1); // TODO: generalize priority management
    ic.enable(irq);
}

/// Dispatch an interrupt to the registered handler.
pub fn dispatch_irq(irq: u32) -> IrqReturn {
    if let Some(handler) = &HANDLERS.lock()[irq as usize] {
        handler.handle()
    } else {
        IrqReturn::Unhandled
    }
}
