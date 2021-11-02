//! A crate for reading and writing Silkroad Online's pk2 archive format.
//!
//! # Examples
//!
//! ```rust
//! use pk2::Pk2;
//! let archive = Pk2::open(archive_path, key)
//!     .unwrap_or_else(|_| panic!("failed to open archive at {:?}", archive_path));
//!
//! ```
//! # Features
//!
//! - `euc-kr`: enabled by default, adds `encoding_rs` as a dependency which changes string reading
//!             and writing to use the `euc-kr` encoding which is required for the original game
//!             archives.
mod blowfish;
mod constants;
mod filetime;
mod io;
mod raw;

mod archive;
pub use self::archive::{fs, Pk2};

mod error;
pub use self::error::{ChainLookupError, ChainLookupResult, InvalidKey, OpenError};
