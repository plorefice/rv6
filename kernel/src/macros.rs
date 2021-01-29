use core::fmt;

/// Prints to the kernel console (UART0).
///
/// Equivalent to the [`kprintln!`] macro except that a newline is not printed
/// at the end of the message.
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::macros::_print(format_args!($($arg)*)));
}

/// Prints to the kernel console (UART0) with a newline (`\n`).
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)+) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub(crate) fn _print(args: fmt::Arguments) {
    use crate::drivers::ns16550::UART0;
    use fmt::Write;

    UART0.lock().write_fmt(args).unwrap();
}
