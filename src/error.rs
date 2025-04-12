use alloc::fmt;

pub use crate::blowfish::InvalidKey;

pub type ChainLookupResult<T> = core::result::Result<T, ChainLookupError>;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ChainLookupError {
    NotFound,
    InvalidPath,
    InvalidChainOffset,
}

#[cfg(feature = "std")]
impl std::error::Error for ChainLookupError {}
impl fmt::Display for ChainLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainLookupError::NotFound => write!(f, "chain not found"),
            ChainLookupError::InvalidPath => write!(f, "invalid path"),
            ChainLookupError::InvalidChainOffset => write!(f, "invalid chain offset"),
        }
    }
}

pub type HeaderResult<T> = core::result::Result<T, HeaderError>;

#[derive(Debug)]
pub enum HeaderError {
    CorruptedFile,
    UnsupportedVersion(u32),
}

#[cfg(feature = "std")]
impl std::error::Error for HeaderError {}
impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::CorruptedFile => write!(f, "archive is invalid or corrupted"),
            HeaderError::UnsupportedVersion(version) => {
                write!(f, "archive version {version} is not supported")
            }
        }
    }
}
