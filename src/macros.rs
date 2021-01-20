/// Prints to the console, which for now is the only UART in the system.
///
/// Equivalent to the [`println!`] macro except that a newline is not printed
/// at the end of the message.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        use crate::drivers::ns16550::*;

        write!(Ns16550::new(NS16550_BASE), $($arg)+).ok();
    }};
}

/// Prints to the console, which for now is the only UART in the system, with a newline (`\n`).
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($fmt:expr) => {
        $crate::print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)+) => {
        $crate::print!(concat!($fmt, "\n"), $($arg)+)
    };
}
