use core::{fmt, io};

use alloc::vec::Vec;

use crate::syscall::sys_write;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let mut total_read = 0;
        while total_read < buf.len() {
            match self.read(&mut buf[total_read..]) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached")),
                Ok(n) => total_read += n,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut total_read = 0;
        let mut temp_buf = [0u8; 1024];
        loop {
            match self.read(&mut temp_buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&temp_buf[..n]);
                    total_read += n;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total_read)
    }
}

pub struct OwnedFd(pub(crate) usize);

pub struct Stdout(OwnedFd);

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match sys_write(self.0.0, buf.as_ptr(), buf.len()) {
            Ok(n) => Ok(n),
            Err(e) => Err(from_raw_os_error(e as isize)),
        }
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

fn from_raw_os_error(code: isize) -> io::Error {
    let kind = match code {
        -22 => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
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
