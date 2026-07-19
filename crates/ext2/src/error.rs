use core::{fmt, io};

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    UnexpectedEof,
    BadMagic,
    InvalidData,
    InvalidInput,
    InvalidFilename,
    NotFound,
    NotADirectory,
    IsADirectory,
    Unsupported,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => e,
            Error::UnexpectedEof => io::ErrorKind::UnexpectedEof.into(),
            Error::BadMagic => io::ErrorKind::InvalidData.into(),
            Error::InvalidData => io::ErrorKind::InvalidData.into(),
            Error::InvalidInput => io::ErrorKind::InvalidInput.into(),
            Error::InvalidFilename => io::ErrorKind::InvalidFilename.into(),
            Error::NotFound => io::ErrorKind::NotFound.into(),
            Error::NotADirectory => io::ErrorKind::NotADirectory.into(),
            Error::IsADirectory => io::ErrorKind::IsADirectory.into(),
            Error::Unsupported => io::ErrorKind::Unsupported.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::UnexpectedEof => write!(f, "unexpected EOF"),
            Error::BadMagic => write!(f, "bad magic number"),
            Error::InvalidData => write!(f, "invalid data"),
            Error::InvalidInput => write!(f, "invalid input"),
            Error::InvalidFilename => write!(f, "invalid filename"),
            Error::NotFound => write!(f, "not found"),
            Error::NotADirectory => write!(f, "not a directory"),
            Error::IsADirectory => write!(f, "is a directory"),
            Error::Unsupported => write!(f, "unsupported operation"),
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}
