//! A crate containing primitives for reading and writing Silkroad Online's pk2 archive format.
//!
//! # Features
//!
//! - `euc-kr`: enabled by default, adds `encoding_rs` as a dependency which changes string reading
//!   and writing to use the `euc-kr` encoding which is required for the original game
//!   archives.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(test), forbid(unsafe_code))]

#[macro_use(vec)]
extern crate alloc;

mod error;
mod filetime;
mod parse;

pub mod blowfish;
mod format;

pub use self::error::{ChainLookupError, ChainLookupResult, HeaderError, InvalidKey};
pub use self::filetime::FILETIME;
pub use self::format::*;
