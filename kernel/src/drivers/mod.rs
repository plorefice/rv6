//! Device and peripheral drivers.

use core::iter::{self, FromIterator};

use alloc::{collections::VecDeque, sync::Arc};
use fdt::{Fdt, FdtParseError, Node, StringList};

pub mod ns16550;
pub mod syscon;
pub mod virtio;

/// A device driver with FDT bindings.
pub trait Driver {
    /// Initializes this driver according to the provided FDT node.
    fn init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<Arc<Self>, DriverError<'d>>
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
    fn _init<'d, 'fdt: 'd>(node: Node<'d, 'fdt>) -> Result<Arc<Self::Driver>, DriverError<'d>> {
        Self::Driver::init(node)
    }
}

/// Type-erased version of the `DriverInfo` trait for dynamic dispatch.
trait DynDriverInfo {
    fn of_match(&self) -> &'static [&'static str];
    fn init<'d, 'fdt: 'd>(&self, node: Node<'d, 'fdt>) -> Result<Arc<dyn Driver>, DriverError<'d>>;
}

impl<T> DynDriverInfo for T
where
    T: DriverInfo,
    T::Driver: Driver + 'static,
{
    fn of_match(&self) -> &'static [&'static str] {
        <T as DriverInfo>::of_match()
    }

    fn init<'d, 'fdt: 'd>(&self, node: Node<'d, 'fdt>) -> Result<Arc<dyn Driver>, DriverError<'d>> {
        Ok(T::Driver::init(node)? as Arc<dyn Driver>)
    }
}

/// Macro helper to generate `DriverInfo` implementations.
#[macro_export]
macro_rules! driver_info {
    {
        type: $type:ty,
        of_match: $of_match:expr,
    } => {
        paste::paste! {
            pub(crate) struct [<$type DriverInfo>];

            impl $crate::drivers::DriverInfo for [<$type DriverInfo>] {
                type Driver = $type;

                fn of_match() -> &'static [&'static str] {
                    &$of_match
                }
            }
        }
    };
}

/// Entry point of the initialization of kernel drivers.
pub fn init<'d>(fdt: &'d Fdt<'d>) -> Result<(), DriverError<'d>> {
    // TODO: global vector with dynamic registration maybe?
    let infos: &[&dyn DynDriverInfo] = &[
        &syscon::GenericSysconDriverInfo,
        &virtio::VirtioMmioDriverInfo,
        &ns16550::Ns16550DriverInfo,
    ];

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
            match modinfo.init(node) {
                Ok(_) | Err(DriverError::DeviceNotFound) => (),
                Err(e) => return Err(e),
            }
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
    /// An unexpected error occurred.
    UnexpectedError(&'d str),
    /// The device could not be found during initialization.
    DeviceNotFound,
}

impl<'d> From<FdtParseError<'d>> for DriverError<'d> {
    fn from(value: FdtParseError<'d>) -> Self {
        Self::Fdt(value)
    }
}
