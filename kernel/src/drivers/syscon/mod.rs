//! System controller drivers.

use alloc::boxed::Box;
use spin::Mutex;

mod generic;

pub use generic::*;

/// Common operations for system controllers.
pub trait Syscon: Sync + Send {
    /// Sends a shutdown signal to the system controller.
    fn poweroff(&self);

    /// Sends a reboot signal to the system controller.
    fn reboot(&self);
}

/// SYSCON functionality provider.
static SYSCON: Mutex<Option<Box<dyn Syscon>>> = Mutex::new(None);

/// Registers a new system controller as global provider.
///
/// If a provider is already registered, it will be replaced.
pub fn register_provider<T>(dev: T)
where
    T: Syscon + 'static,
{
    let mut syscon = SYSCON.lock();

    if syscon.is_none() {
        *syscon = Some(Box::new(dev));
    } else {
        kprintln!("Error: only one system controller is supported!")
    }
}

/// Powers off the system.
pub fn poweroff() {
    if let Some(syscon) = SYSCON.lock().as_ref() {
        syscon.poweroff();
    } else {
        kprintln!("System shutdown failed: no system controller registered");
    }
}

/// Reboots the system.
pub fn reboot() {
    if let Some(syscon) = SYSCON.lock().as_ref() {
        syscon.reboot();
    } else {
        kprintln!("System reboot failed: no system controller registered");
    }
}
