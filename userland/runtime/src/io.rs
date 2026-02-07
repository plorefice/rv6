use core::fmt;

use crate::syscall::sys_write;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    code: isize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error code: {}", self.code)
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    fn flush(&mut self) -> Result<()>;
}

pub struct Fd(usize);

pub struct Stdout(Fd);

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = sys_write(self.0.0, buf.as_ptr(), buf.len());
        if n < 0 {
            Err(Error { code: n })
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

pub fn stdout() -> Stdout {
    Stdout(Fd(1))
}
