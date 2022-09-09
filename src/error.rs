use std::{error, fmt, io};

pub use crate::blowfish::InvalidKey;

pub type ChainLookupResult<T> = Result<T, ChainLookupError>;
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ChainLookupError {
    NotFound,
    InvalidPath,
    InvalidChainIndex,
    ExpectedDirectory,
    ExpectedFile,
}

impl error::Error for ChainLookupError {}
impl fmt::Display for ChainLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&io::Error::from(*self), f)
    }
}

impl From<ChainLookupError> for io::Error {
    #[inline]
    fn from(this: ChainLookupError) -> Self {
        match this {
            ChainLookupError::NotFound => io::ErrorKind::NotFound,
            ChainLookupError::InvalidPath => io::ErrorKind::InvalidInput,
            ChainLookupError::InvalidChainIndex => io::ErrorKind::InvalidData,
            ChainLookupError::ExpectedDirectory => io::ErrorKind::NotFound,
            ChainLookupError::ExpectedFile => io::ErrorKind::NotFound,
        }
        .into()
    }
}

pub type OpenResult<T> = std::result::Result<T, OpenError>;

#[derive(Debug)]
pub enum OpenError {
    InvalidKey,
    CorruptedFile,
    UnsupportedVersion,
    Io(io::Error),
}

impl error::Error for OpenError {}
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
