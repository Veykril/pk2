use std::{fmt, io};

pub type Pk2Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    ExpectedFile,
    ExpectedDirectory,
    NonUnicodePath,
    InvalidKey,
    InvalidPath,
    InvalidChainIndex,
    CorruptedFile,
    UnsupportedVersion,
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
            Error::InvalidKey => write!(f, "blowfish key was invalid"),
            Error::InvalidPath => write!(f, "path was invalid"),
            Error::CorruptedFile => write!(f, "archive is invalid or corrupted"),
            Error::UnsupportedVersion => write!(f, "archive version is not supported"),
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
