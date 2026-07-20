use core::{fmt, io};

use alloc::io::Read;
use spin::{Mutex, MutexGuard, Once};

use crate::syscall::{sys_read, sys_write};

pub struct OwnedFd(pub(crate) usize);

pub struct Stdout(OwnedFd);

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        sys_write(self.0.0, buf.as_ptr(), buf.len()).map_err(io::Error::from)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        io::Write::write(self, s.as_bytes())
            .map(|_| ())
            .map_err(|_| fmt::Error)
    }
}

pub fn stdout() -> Stdout {
    Stdout(OwnedFd(1))
}

pub struct Stdin {
    inner: &'static Mutex<OwnedFd>,
}

pub struct StdinLock<'a> {
    inner: MutexGuard<'a, OwnedFd>,
}

pub fn stdin() -> Stdin {
    static INSTANCE: Once<Mutex<OwnedFd>> = Once::new();
    Stdin {
        inner: INSTANCE.call_once(|| Mutex::new(OwnedFd(0))),
    }
}

impl Stdin {
    pub fn lock(&self) -> StdinLock<'static> {
        StdinLock {
            inner: self.inner.lock(),
        }
    }
}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.lock().read(buf)
    }
}

impl Read for StdinLock<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        sys_read(self.inner.0, buf.as_mut_ptr(), buf.len()).map_err(io::Error::from)
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::io::_print(core::format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n");
    };
    ($($arg:tt)*) => {
        $crate::io::_print(core::format_args!("{}\n", core::format_args!($($arg)*)));
    };
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut out = stdout();
    out.write_fmt(args).unwrap();
}
