/// Prints to the kernel console (UART0).
///
/// Equivalent to the [`kprintln!`] macro except that a newline is not printed
/// at the end of the message.
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        use crate::drivers::ns16550::*;

        write!(Ns16550::new(NS16550_BASE), $($arg)+).ok();
    }};
}

/// Prints to the kernel console (UART0) with a newline (`\n`).
#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::kprint!("\n")
    };
    ($fmt:expr) => {
        $crate::kprint!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)+) => {
        $crate::kprint!(concat!($fmt, "\n"), $($arg)+)
    };
}
