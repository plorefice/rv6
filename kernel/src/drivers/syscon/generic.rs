//! Generic system controller.

use fdt::{Fdt, Node, PropEncodedArray};

use crate::{
    driver_info,
    drivers::{syscon, Driver, DriverError},
    mm::mmio::Regmap,
};

driver_info! {
    type: GenericSyscon,
    of_match: ["syscon"],
}

/// Generic system controller.
pub struct GenericSyscon {
    regmap: Regmap,
    poweroff: Option<SysconRegister>,
    reboot: Option<SysconRegister>,
}

struct SysconRegister {
    offset: usize,
    value: u32,
    _mask: u32,
}

impl Driver for GenericSyscon {
    fn init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<(), DriverError<'d>> {
        let mut regs = node
            .property::<PropEncodedArray<(u64, u64)>>("reg")
            .ok_or(DriverError::MissingRequiredProperty("reg"))?;

        let (base, len) = regs.next().expect("empty reg property");

        // SAFETY: assuming the node contains a valid regmap
        let regmap = unsafe { Regmap::new(base, len) };

        let mut slf = Self {
            regmap,
            poweroff: None,
            reboot: None,
        };

        // Find poweroff and reboot nodes
        if let Some(phandle) = node.property::<u32>("phandle") {
            slf.poweroff = find_syscon_driver(node.fdt(), phandle, "syscon-poweroff")?;
            slf.reboot = find_syscon_driver(node.fdt(), phandle, "syscon-reboot")?;
        }

        syscon::register_provider(slf);

        Ok(())
    }
}

impl super::Syscon for GenericSyscon {
    fn poweroff(&self) {
        if let Some(ref reg) = self.poweroff {
            self.regmap.write::<u32>(reg.offset, reg.value);
        }
    }

    fn reboot(&self) {
        if let Some(ref reg) = self.reboot {
            self.regmap.write::<u32>(reg.offset, reg.value);
        }
    }
}

fn find_syscon_driver<'d>(
    fdt: &'d Fdt,
    phandle: u32,
    compatible: &str,
) -> Result<Option<SysconRegister>, DriverError<'d>> {
    Ok(fdt
        .find_compatible(compatible)?
        .filter(|n| n.property::<u32>("regmap") == Some(phandle))
        .and_then(parse_syscon_driver_node))
}

fn parse_syscon_driver_node(node: Node) -> Option<SysconRegister> {
    let offset = node.property::<u32>("offset")? as usize;
    let value = node.property::<u32>("value")?;
    let mask = node.property::<u32>("mask").unwrap_or(u32::MAX);

    Some(SysconRegister {
        offset,
        value,
        _mask: mask,
    })
}
