use std::{fmt, io};

pub use crate::blowfish::InvalidKey;

pub type Pk2Result<T> = std::result::Result<T, Error>;
pub type OpenResult<T> = std::result::Result<T, OpenError>;

pub enum OpenError {
    InvalidKey,
    CorruptedFile,
    UnsupportedVersion,
    Io(io::Error),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::CorruptedFile => write!(f, "archive is invalid or corrupted"),
            OpenError::UnsupportedVersion => write!(f, "archive version is not supported"),
            OpenError::InvalidKey => write!(f, "blowfish key was invalid"),
            OpenError::Io(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl From<io::Error> for OpenError {
    #[inline]
    fn from(e: io::Error) -> Self {
        OpenError::Io(e)
    }
}

impl From<InvalidKey> for OpenError {
    #[inline]
    fn from(_: InvalidKey) -> Self {
        OpenError::InvalidKey
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    ExpectedFile,
    ExpectedDirectory,
    NonUnicodePath,
    InvalidPath,
    InvalidChainIndex,
    NotFound,
    AlreadyExists,
}

impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => fmt::Display::fmt(e, f),
            Error::ExpectedFile => write!(f, "expected file entry, found directory"),
            Error::ExpectedDirectory => write!(f, "expected directory entry, found file"),
            Error::NonUnicodePath => write!(f, "couldn't interpret path as a unicode string slice"),
            Error::InvalidPath => write!(f, "path was invalid"),
            Error::InvalidChainIndex => {
                write!(f, "archive contains an invalid chain index reference")
            }
            Error::NotFound => write!(f, "file or directory not found"),
            Error::AlreadyExists => write!(f, "path already exists"),
        }
    }
}

impl From<io::Error> for Error {
    #[inline]
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
