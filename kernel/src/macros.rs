//! Utility macros.

use core::fmt;

/// Prints to the kernel console (UART0).
///
/// Equivalent to the [`kprintln!`] macro except that a newline is not printed
/// at the end of the message.
#[macro_export]
macro_rules! kprint {
    () => ($crate::macros::_print_timestamp());
    ($($arg:tt)*) => ({
        $crate::macros::_print_timestamp();
        $crate::macros::_print(format_args!($($arg)*));
    });
}

/// Prints to the kernel console (UART0) with a newline (`\n`).
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)+) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

/// Prints a line continuation to the kernel console.
///
/// No extra characters (eg. timestamp) will be prepended to the line.
#[macro_export]
macro_rules! kprintc {
    ($($arg:tt)*) => ($crate::macros::_print(format_args!($($arg)*)));
}

/// Prints a line termination to the kernel console.
///
/// Meant to be used after [`kprint!`] or [`kprintc!`] to terminate a line correctly.
#[macro_export]
macro_rules! kprinte {
    () => {
        $crate::kprintc!("\n")
    };
}

/// Prints and returns the value of a given expression for quick and dirty
/// debugging.
#[macro_export]
macro_rules! kdbg {
    () => {
        $crate::kprintln!("[{}:{}:{}]", core::file!(), core::line!(), core::column!())
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::kprintln!("[{}:{}:{}] {} = {:#?}",
                    core::file!(), core::line!(), core::column!(), core::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

#[doc(hidden)]
pub(crate) fn _print(args: fmt::Arguments) {
    use fmt::Write;

    crate::drivers::earlycon::get().write_fmt(args).unwrap();
}

#[doc(hidden)]
pub(crate) fn _print_timestamp() {
    use crate::arch::time;

    let cy = time::get_cycles();
    let sec = cy / time::CLINT_TIMEBASE;
    let subsec = (cy % time::CLINT_TIMEBASE) / 10;

    _print(format_args!("[{sec:5}.{subsec:06}] "));
}
