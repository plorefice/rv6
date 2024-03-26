//! Device and peripheral drivers.

use core::iter::{self, FromIterator};

use alloc::{boxed::Box, collections::VecDeque};
use fdt::{Fdt, FdtParseError, Node, StringList};

pub mod ns16550;
pub mod syscon;

/// A device driver with FDT bindings.
pub trait Driver {
    /// Initializes this driver according to the provided FDT node.
    fn init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<Self, DriverError<'d>>
    where
        Self: Sized;
}

/// Driver information required for creation.
pub trait DriverInfo {
    /// The driver to which these info refer.
    type Driver: Driver;

    /// Returns a list of strings that match an FDT node's "compatible" property.
    fn of_match() -> &'static [&'static str];

    /// Calls the `Self::Driver::init` function passing on the FDT node.
    ///
    /// Implementation is provided, so no need to override it.
    fn _init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<Self::Driver, DriverError<'d>> {
        Self::Driver::init(node)
    }
}

/// Type-erased version of the `DriverInfo` trait for dynamic dispatch.
trait DynDriverInfo {
    fn of_match(&self) -> &'static [&'static str];
    fn init<'d, 'fdt: 'd>(&self, node: Node<'d, 'fdt>) -> Result<Box<dyn Driver>, DriverError<'d>>;
}

impl<T> DynDriverInfo for T
where
    T: DriverInfo,
    T::Driver: Driver + 'static,
{
    fn of_match(&self) -> &'static [&'static str] {
        <T as DriverInfo>::of_match()
    }

    fn init<'d, 'fdt: 'd>(&self, node: Node<'d, 'fdt>) -> Result<Box<dyn Driver>, DriverError<'d>> {
        Ok(Box::new(T::Driver::init(node)?) as Box<dyn Driver>)
    }
}

/// Entry point of the initialization of kernel drivers.
pub fn init<'d>(fdt: &'d Fdt<'d>) -> Result<(), DriverError<'d>> {
    // TODO: global vector with dynamic registration maybe?
    let infos: &[&dyn DynDriverInfo] = &[&syscon::GenericSysconDriverInfo];

    let mut nodes = VecDeque::from_iter(iter::once(fdt.root_node()?));

    while let Some(node) = nodes.pop_front() {
        nodes.extend(node.children());

        let Some(compatibles) = node.property::<StringList>("compatible") else {
            continue;
        };

        if let Some(modinfo) = infos
            .iter()
            .find(|i| compatibles.clone().any(|c| i.of_match().contains(&c)))
        {
            modinfo.init(node)?;
        }
    }

    Ok(())
}

/// Driver-related errors.
#[derive(Debug)]
pub enum DriverError<'d> {
    /// An error occurred while accessing an FDT node.
    Fdt(FdtParseError<'d>),
    /// The FDT node did not contain a necessary property for driver initialization.
    MissingRequiredProperty(&'d str),
}

impl<'d> From<FdtParseError<'d>> for DriverError<'d> {
    fn from(value: FdtParseError<'d>) -> Self {
        Self::Fdt(value)
    }
}
