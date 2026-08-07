//! QEMU-specific drivers.

mod fw_cfg;
mod ramfb;

pub use fw_cfg::*;
pub use ramfb::*;
