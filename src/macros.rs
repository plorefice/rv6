#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {};
}

#[macro_export]
macro_rules! println {
    () => {
        print!("\n")
    };
    ($fmt:expr) => {
        print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($args:tt)+) => {
        print!(concat!($fmt, "\n"), $($args)+)
    };
}
