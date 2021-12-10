//! A crate for reading and writing Silkroad Online's pk2 archive format.
//!
//! # Examples
//!
//! ```rust,no_run
//! # let archive_path = "";
//! # let key = b"";
//! use pk2::unsync::Pk2;
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
pub use self::archive::fs::{DirEntry, Directory, File, FileMut};
pub use self::archive::Pk2;

mod error;
pub use self::error::{ChainLookupError, ChainLookupResult, InvalidKey, OpenError};

pub struct ReadOnly<B>(pub B);
impl<B: std::io::Read> std::io::Read for ReadOnly<B> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}
impl<B: std::io::Seek> std::io::Seek for ReadOnly<B> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

pub trait BufferAccess<B> {
    fn new(b: B) -> Self;
    fn into_inner(self) -> B;
    fn with_mut_buffer<R>(&self, f: impl FnOnce(&mut B) -> R) -> R;
}

macro_rules! gen_type_aliases {
    ($ident:ident) => {
        pub type Pk2<Buffer = std::fs::File> = crate::archive::Pk2<Buffer, $ident<Buffer>>;

        pub type File<'pk2, Buffer = std::fs::File> =
            crate::archive::fs::File<'pk2, Buffer, $ident<Buffer>>;
        pub type FileMut<'pk2, Buffer = std::fs::File> =
            crate::archive::fs::FileMut<'pk2, Buffer, $ident<Buffer>>;
        pub type DirEntry<'pk2, Buffer = std::fs::File> =
            crate::archive::fs::DirEntry<'pk2, Buffer, $ident<Buffer>>;
        pub type Directory<'pk2, Buffer = std::fs::File> =
            crate::archive::fs::Directory<'pk2, Buffer, $ident<Buffer>>;
        pub mod readonly {
            pub type Pk2<Buffer = std::fs::File> = super::Pk2<crate::ReadOnly<Buffer>>;

            pub type File<'pk2, Buffer = std::fs::File> =
                super::File<'pk2, crate::ReadOnly<Buffer>>;
            pub type FileMut<'pk2, Buffer = std::fs::File> =
                super::FileMut<'pk2, crate::ReadOnly<Buffer>>;
            pub type DirEntry<'pk2, Buffer = std::fs::File> =
                super::DirEntry<'pk2, crate::ReadOnly<Buffer>>;
            pub type Directory<'pk2, Buffer = std::fs::File> =
                super::Directory<'pk2, crate::ReadOnly<Buffer>>;
        }
    };
}

pub mod sync {
    use std::sync::Mutex;

    gen_type_aliases! {
        Mutex
    }

    impl<B> crate::BufferAccess<B> for Mutex<B> {
        fn new(b: B) -> Self {
            Mutex::new(b)
        }
        fn into_inner(self) -> B {
            self.into_inner().unwrap()
        }
        fn with_mut_buffer<R>(&self, f: impl FnOnce(&mut B) -> R) -> R {
            f(&mut self.lock().unwrap())
        }
    }
}

pub mod unsync {
    use std::cell::RefCell;

    gen_type_aliases! {
        RefCell
    }

    impl<B> crate::BufferAccess<B> for RefCell<B> {
        fn new(b: B) -> Self {
            RefCell::new(b)
        }
        fn into_inner(self) -> B {
            self.into_inner()
        }
        fn with_mut_buffer<R>(&self, f: impl FnOnce(&mut B) -> R) -> R {
            f(&mut self.borrow_mut())
        }
    }
}
