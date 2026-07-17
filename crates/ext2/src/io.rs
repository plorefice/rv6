use core::io;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), io::Error> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(io::ErrorKind::UnexpectedEof.into())
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "alloc")]
    fn read_to_end(&mut self, buf: &mut alloc::vec::Vec<u8>) -> Result<usize, io::Error> {
        let mut total_read = 0;
        loop {
            let mut temp_buf = [0u8; 1024];
            match self.read(&mut temp_buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&temp_buf[..n]);
                    total_read += n;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(total_read)
    }
}

#[cfg(feature = "std")]
mod std_io {
    use std::io;

    use super::*;

    #[doc(hidden)]
    pub struct StdIoWrapper<T> {
        inner: T,
    }

    impl<T> From<T> for StdIoWrapper<T> {
        fn from(inner: T) -> Self {
            Self::new(inner)
        }
    }

    impl<T> StdIoWrapper<T> {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }

        pub fn into_inner(self) -> T {
            self.inner
        }
    }

    impl<T: io::Read> Read for StdIoWrapper<T> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
            self.inner.read(buf)
        }

        fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), io::Error> {
            self.inner.read_exact(buf)
        }
    }

    impl<T: io::Write> io::Write for StdIoWrapper<T> {
        fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
            self.inner.write(buf)
        }

        fn flush(&mut self) -> Result<(), io::Error> {
            self.inner.flush()
        }
    }

    impl<T: io::Seek> io::Seek for StdIoWrapper<T> {
        fn seek(&mut self, pos: io::SeekFrom) -> Result<u64, io::Error> {
            self.inner.seek(pos)
        }
    }
}

#[cfg(feature = "std")]
pub use std_io::StdIoWrapper;
