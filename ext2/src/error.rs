#[derive(Debug)]
#[non_exhaustive]
pub enum Error<T> {
    Io(T),
    UnexpectedEof,
    WriteZero,
}

impl<T: IoError> From<T> for Error<T> {
    fn from(e: T) -> Self {
        Self::Io(e)
    }
}

#[cfg(feature = "std")]
impl From<Error<std::io::Error>> for std::io::Error {
    fn from(e: Error<Self>) -> Self {
        match e {
            Error::Io(e) => e,
            Error::UnexpectedEof => Self::new(std::io::ErrorKind::UnexpectedEof, e),
            Error::WriteZero => Self::new(std::io::ErrorKind::WriteZero, e),
        }
    }
}

impl<T: core::fmt::Display> core::fmt::Display for Error<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::UnexpectedEof => write!(f, "unexpected EOF"),
            Error::WriteZero => write!(f, "write zero"),
        }
    }
}

#[cfg(feature = "std")]
impl<T: std::error::Error + 'static> std::error::Error for Error<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

pub trait IoError: core::fmt::Debug {
    fn is_interrupted(&self) -> bool;
    fn new_unexpected_eof_error() -> Self;
    fn new_write_zero_error() -> Self;
}

impl<T: core::fmt::Debug + IoError> IoError for Error<T> {
    fn is_interrupted(&self) -> bool {
        match self {
            Error::Io(e) => e.is_interrupted(),
            _ => false,
        }
    }

    fn new_unexpected_eof_error() -> Self {
        Self::UnexpectedEof
    }

    fn new_write_zero_error() -> Self {
        Self::WriteZero
    }
}

#[cfg(feature = "std")]
impl IoError for std::io::Error {
    fn is_interrupted(&self) -> bool {
        self.kind() == std::io::ErrorKind::Interrupted
    }

    fn new_unexpected_eof_error() -> Self {
        Self::new(std::io::ErrorKind::UnexpectedEof, "unexpected EOF")
    }

    fn new_write_zero_error() -> Self {
        Self::new(
            std::io::ErrorKind::WriteZero,
            "failed to write whole buffer",
        )
    }
}
