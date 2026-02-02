use crate::error::IoError;

pub trait IoBase {
    type Error: IoError;
}

pub trait Read: IoBase {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), Self::Error> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.is_interrupted() => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Self::Error::new_unexpected_eof_error())
        } else {
            Ok(())
        }
    }
}

pub trait Write: IoBase {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error>;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<(), Self::Error> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    return Err(Self::Error::new_write_zero_error());
                }
                Ok(n) => buf = &buf[n..],
                Err(ref e) if e.is_interrupted() => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

pub trait Seek: IoBase {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error>;
}

#[cfg(feature = "std")]
impl From<SeekFrom> for std::io::SeekFrom {
    fn from(sf: SeekFrom) -> Self {
        match sf {
            SeekFrom::Start(s) => std::io::SeekFrom::Start(s),
            SeekFrom::End(s) => std::io::SeekFrom::End(s),
            SeekFrom::Current(s) => std::io::SeekFrom::Current(s),
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::SeekFrom> for SeekFrom {
    fn from(sf: std::io::SeekFrom) -> Self {
        match sf {
            std::io::SeekFrom::Start(s) => SeekFrom::Start(s),
            std::io::SeekFrom::End(s) => SeekFrom::End(s),
            std::io::SeekFrom::Current(s) => SeekFrom::Current(s),
        }
    }
}

#[doc(hidden)]
#[cfg(feature = "std")]
pub struct StdIoWrapper<T> {
    inner: T,
}

#[cfg(feature = "std")]
impl<T> StdIoWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[cfg(feature = "std")]
impl<T> IoBase for StdIoWrapper<T> {
    type Error = std::io::Error;
}

#[cfg(feature = "std")]
impl<T: std::io::Read> Read for StdIoWrapper<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.inner.read(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Self::Error> {
        self.inner.read_exact(buf)
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Write> Write for StdIoWrapper<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.inner.write(buf)
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_all(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Seek> Seek for StdIoWrapper<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.inner.seek(pos.into())
    }
}

#[cfg(feature = "std")]
impl<T> From<T> for StdIoWrapper<T> {
    fn from(inner: T) -> Self {
        Self::new(inner)
    }
}
