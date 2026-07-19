//! Console device management.
//!
//! This module is a temporary solution to manage the console device in a global context.
//! In the future, it will probably be replaced by a more robust device management system.

use alloc::sync::Arc;
use spin::Once;

use crate::vfs::file_ops::FileOps;

static CONSOLE: Once<Arc<dyn FileOps>> = Once::new();

/// Registers `console` as the global console device.
pub fn register(console: Arc<dyn FileOps>) {
    CONSOLE.call_once(|| console);
}

/// Returns the global console device.
///
/// # Panics
///
/// This function panics if the console device has not been initialized yet.
pub fn get() -> Arc<dyn FileOps> {
    CONSOLE.get().expect("console not initialized").clone()
}
