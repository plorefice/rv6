#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        use core::fmt::Write;
        use crate::drivers::ns16550::*;

        write!(Ns16550::new(NS16550_BASE), $($arg)+).ok();
    };
}

#[macro_export]
macro_rules! println {
    () => {
        print!("\n")
    };
    ($fmt:expr) => {
        print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)+) => {
        print!(concat!($fmt, "\n"), $($arg)+)
    };
}
