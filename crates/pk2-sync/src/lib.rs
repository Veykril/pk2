pub mod fs;
use self::fs::{Directory, File, FileMut};

mod io;

use std::io::{Cursor, Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::path::Path;
use std::{fs as stdfs, io as stdio};

use pk2::block_chain::PackBlock;
use pk2::blowfish::Blowfish;
use pk2::chain_index::ChainIndex;
use pk2::entry::PackEntry;
use pk2::header::PackHeader;
use pk2::{ChainOffset, StreamOffset};

/// An IO wrapper type that only exposes read and seek operations.
pub struct ReadOnly<B>(pub B);
impl<B: stdio::Read> stdio::Read for ReadOnly<B> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.0.read(buf)
    }
}
impl<B: stdio::Seek> stdio::Seek for ReadOnly<B> {
    fn seek(&mut self, pos: stdio::SeekFrom) -> IoResult<u64> {
        self.0.seek(pos)
    }
}

/// A type that allows mutable access to its inner value via interior mutability.
pub trait Lock<T> {
    /// Create a new instance of the lock.
    fn new(b: T) -> Self;
    /// Consume the lock and return the inner value.
    fn into_inner(self) -> T;
    /// Perform an operation on the inner value by taking the lock.
    fn with_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
}

/// A type that allows choosing between different locking mechanisms for the backing buffer of the
/// pk2 archive.
pub trait LockChoice {
    /// The type of lock to be used.
    type Lock<T>: Lock<T>;
    /// Wrap the value in our lock.
    fn new_locked<T>(t: T) -> Self::Lock<T> {
        Self::Lock::new(t)
    }
}

macro_rules! gen_type_aliases {
    ($lock:ident) => {
        pub type Pk2<Buffer = std::fs::File> = crate::Pk2<Buffer, $lock>;

        pub type File<'pk2, Buffer = std::fs::File> = crate::fs::File<'pk2, Buffer, $lock>;
        pub type FileMut<'pk2, Buffer = std::fs::File> = crate::fs::FileMut<'pk2, Buffer, $lock>;
        pub type DirEntry<'pk2, Buffer = std::fs::File> = crate::fs::DirEntry<'pk2, Buffer, $lock>;
        pub type Directory<'pk2, Buffer = std::fs::File> =
            crate::fs::Directory<'pk2, Buffer, $lock>;
        /// Read-only versions of the API types.
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

pub use self::sync::Lock as SyncLock;
pub mod sync {
    use std::sync::Mutex;

    /// A lock that uses a [`std::sync::Mutex`] to provide interior mutability.
    pub enum Lock {}
    impl super::LockChoice for Lock {
        type Lock<T> = Mutex<T>;
    }

    gen_type_aliases! {
        Lock
    }

    impl<T> super::Lock<T> for Mutex<T> {
        fn new(b: T) -> Self {
            Mutex::new(b)
        }
        fn into_inner(self) -> T {
            self.into_inner().unwrap()
        }
        fn with_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
            f(&mut self.lock().unwrap())
        }
    }
}

pub use self::unsync::Lock as UnsyncLock;
pub mod unsync {
    use std::cell::RefCell;

    /// A lock that uses a [`std::cell::RefCell`] to provide interior mutability.
    pub enum Lock {}
    impl super::LockChoice for Lock {
        type Lock<T> = RefCell<T>;
    }

    gen_type_aliases! {
        Lock
    }

    impl<T> super::Lock<T> for RefCell<T> {
        fn new(b: T) -> Self {
            RefCell::new(b)
        }
        fn into_inner(self) -> T {
            self.into_inner()
        }
        fn with_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
            f(&mut self.borrow_mut())
        }
    }
}

use IoResult as OpenResult;

/// A Pk2 archive.
pub struct Pk2<Buffer, L: LockChoice> {
    stream: <L as LockChoice>::Lock<Buffer>,
    blowfish: Option<Box<Blowfish>>,
    chain_index: ChainIndex,
    유령: PhantomData<Buffer>,
}

impl<L: LockChoice> Pk2<stdfs::File, L> {
    /// Creates a new [`File`](stdfs::File) based archive at the given path.
    pub fn create_new<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path.as_ref())?;
        Self::_create_impl(file, key)
    }

    /// Opens an archive at the given path.
    ///
    /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    /// operations on the file making this operation potentially slow.
    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new().write(true).read(true).open(path)?;
        Self::_open_in_impl(file, key)
    }
}

impl<L: LockChoice> Pk2<ReadOnly<stdfs::File>, L> {
    /// Opens an archive at the given path.
    ///
    /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    /// operations on the file making this operation potentially slow.
    pub fn open_readonly<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new().write(true).read(true).open(path)?;
        Self::_open_in_impl(ReadOnly(file), key)
    }

    // /// Opens an archive at the given path with its file index sorted.
    // ///
    // /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    // /// operations on the file making this operation potentially slow.
    // pub fn open_sorted<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
    //     let file = stdfs::OpenOptions::new().read(true).open(path)?;
    //     let mut this = Self::_open_in_impl(ReadOnly(file), key)?;
    //     this.chain_index.sort();
    //     Ok(this)
    // }
}

impl<L: LockChoice> Pk2<Cursor<Vec<u8>>, L> {
    /// Creates a new archive in memory.
    pub fn create_new_in_memory<K: AsRef<[u8]>>(key: K) -> Result<Self, pk2::blowfish::InvalidKey> {
        Self::_create_impl(Cursor::new(Vec::with_capacity(4096)), key).map_err(|_| {
            // the only error that can actually occur here is an InvalidKey error
            pk2::blowfish::InvalidKey
        })
    }
}

impl<L: LockChoice> From<Pk2<Cursor<Vec<u8>>, L>> for Vec<u8> {
    fn from(pk2: Pk2<Cursor<Vec<u8>>, L>) -> Self {
        pk2.stream.into_inner().into_inner()
    }
}

impl<B, L> Pk2<B, L>
where
    B: stdio::Read + stdio::Seek,
    L: LockChoice,
{
    /// Opens an archive from the given stream.
    ///
    /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    /// operations on the stream.
    pub fn open_in<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        stream.seek(stdio::SeekFrom::Start(0))?;
        Self::_open_in_impl(stream, key)
    }

    fn _open_in_impl<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        let mut buffer = [0; PackHeader::PACK_HEADER_LEN];
        stream.read_exact(&mut buffer)?;
        let header = PackHeader::parse(&buffer);
        header.validate_sig().map_err(|e| IoError::new(IoErrorKind::InvalidData, e))?;
        let blowfish = if header.encrypted {
            let bf = Blowfish::new(key.as_ref())
                .map_err(|e| IoError::new(IoErrorKind::InvalidInput, e))?;
            header.verify(&bf).map_err(|e| IoError::new(IoErrorKind::InvalidInput, e))?;
            Some(Box::new(bf))
        } else {
            None
        };
        let chain_index = ChainIndex::read_sync(&mut stream, blowfish.as_deref())?;

        Ok(Pk2 {
            stream: <L as LockChoice>::Lock::new(stream),
            blowfish,
            chain_index,
            유령: PhantomData,
        })
    }
}

impl<B, L> Pk2<B, L>
where
    B: stdio::Read + stdio::Write + stdio::Seek,
    L: LockChoice,
{
    pub fn create_new_in<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        stream.seek(stdio::SeekFrom::Start(0))?;
        Self::_create_impl(stream, key)
    }

    fn _create_impl<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        let (header, blowfish) = if key.as_ref().is_empty() {
            (PackHeader::default(), None)
        } else {
            let bf = Blowfish::new(key.as_ref())
                .map_err(|e| IoError::new(IoErrorKind::InvalidInput, e))?;
            (PackHeader::new_encrypted(&bf), Some(Box::new(bf)))
        };

        let mut out = [0; PackHeader::PACK_HEADER_LEN];
        header.write_into(&mut out);
        stream.write_all(&out)?;
        let mut block = PackBlock::default();
        block[0] = PackEntry::new_directory(".", ChainIndex::PK2_ROOT_CHAIN_OFFSET, None);
        crate::io::write_block(
            blowfish.as_deref(),
            &mut stream,
            ChainIndex::PK2_ROOT_BLOCK_OFFSET,
            &block,
        )?;

        let chain_index = ChainIndex::read_sync(&mut stream, blowfish.as_deref())?;
        Ok(Pk2 { stream: L::new_locked(stream), blowfish, chain_index, 유령: PhantomData })
    }
}

impl<L: LockChoice, B> Pk2<B, L> {
    fn root_resolve_path_to_entry_and_parent<P: AsRef<str>>(
        &self,
        path: P,
    ) -> OpenResult<Option<(ChainOffset, usize, &PackEntry)>> {
        let path = check_root(path.as_ref())?;
        if path.is_empty() {
            return Ok(None);
        }
        self.chain_index
            .resolve_path_to_entry_and_parent(None, path)
            .map(Some)
            .map_err(|e| IoError::new(IoErrorKind::InvalidData, e))
    }

    fn root_resolve_path_to_entry_and_parent_mut<P: AsRef<str>>(
        &mut self,
        path: P,
    ) -> OpenResult<Option<(ChainOffset, usize, &mut PackEntry)>> {
        let path = check_root(path.as_ref())?;
        if path.is_empty() {
            return Ok(None);
        }
        self.chain_index
            .resolve_path_to_entry_and_parent_mut(None, path)
            .map(Some)
            .map_err(|e| IoError::new(IoErrorKind::InvalidData, e))
    }

    fn is_file(entry: &PackEntry) -> OpenResult<()> {
        match entry.is_file() {
            true => Ok(()),
            false => Err(IoError::new(IoErrorKind::InvalidData, "Expected a file entry")),
        }
    }

    fn is_dir(entry: &PackEntry) -> OpenResult<()> {
        match entry.is_directory() {
            true => Ok(()),
            false => Err(IoError::new(IoErrorKind::InvalidData, "Expected a directory entry")),
        }
    }
}

impl<B, L: LockChoice> Pk2<B, L> {
    pub fn open_file<P: AsRef<str>>(&self, path: P) -> OpenResult<File<B, L>> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)?
            .ok_or_else(|| IoError::new(IoErrorKind::InvalidData, "Expected a file entry"))?;
        Self::is_file(entry)?;
        Ok(File::new(self, chain, entry_idx))
    }

    pub fn open_directory<P: AsRef<str>>(&self, path: P) -> OpenResult<Directory<B, L>> {
        match self.root_resolve_path_to_entry_and_parent(path)? {
            Some((chain, entry_idx, entry)) => {
                Self::is_dir(entry)?;
                Ok(Directory::new(self, Some(chain), entry_idx))
            }
            None => Ok(Directory::new(self, None, 0)),
        }
    }

    pub fn open_root_dir(&self) -> Directory<B, L> {
        Directory::new(self, None, 0)
    }

    /// Invokes cb on every file in the sub directories of `base`, including
    /// files inside of its subdirectories. Cb gets invoked with its
    /// relative path to `base` and the file object.
    pub fn for_each_file(
        &self,
        base: impl AsRef<str>,
        cb: impl FnMut(&Path, File<'_, B, L>) -> OpenResult<()>,
    ) -> OpenResult<()> {
        self.open_directory(base)?.for_each_file(cb)
    }
}

impl<B, L> Pk2<B, L>
where
    B: stdio::Read + stdio::Seek,
    L: LockChoice,
{
    pub fn read<P: AsRef<str>>(&self, path: P) -> OpenResult<Vec<u8>> {
        let mut file = self.open_file(path)?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        stdio::Read::read_to_end(&mut file, &mut buf)?;
        Ok(buf)
    }
}

impl<B, L> Pk2<B, L>
where
    B: stdio::Read + stdio::Write + stdio::Seek,
    L: LockChoice,
{
    pub fn open_file_mut<P: AsRef<str>>(&mut self, path: P) -> OpenResult<FileMut<B, L>> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)?
            .ok_or_else(|| IoError::new(IoErrorKind::InvalidData, "Expected a file entry"))?;
        Self::is_file(entry)?;
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// Currently only replaces the entry with an empty one making the data
    /// inaccessible by normal means
    pub fn delete_file<P: AsRef<str>>(&mut self, path: P) -> OpenResult<()> {
        let (chain_index, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent_mut(path)?
            .ok_or_else(|| IoError::new(IoErrorKind::InvalidData, "Expected a file entry"))?;
        Self::is_file(entry)?;
        entry.clear();

        self.stream.with_lock(|stream| {
            crate::io::write_chain_entry(
                self.blowfish.as_deref(),
                stream,
                self.chain_index.get(chain_index).unwrap(),
                entry_idx,
            )
        })?;
        Ok(())
    }

    pub fn create_file<P: AsRef<str>>(&mut self, path: P) -> OpenResult<FileMut<B, L>> {
        let path = check_root(path.as_ref())?;
        let (chain, entry_idx, file_name) = self.stream.with_lock(|stream| {
            Self::create_entry_at(
                &mut self.chain_index,
                self.blowfish.as_deref(),
                stream,
                ChainIndex::PK2_ROOT_CHAIN_OFFSET,
                path,
            )
        })?;
        let entry = self.chain_index.get_entry_mut(chain, entry_idx).unwrap();
        // The stream offset is a dummy value
        *entry = PackEntry::new_file(
            file_name,
            StreamOffset(ChainIndex::PK2_ROOT_BLOCK_OFFSET.0),
            0,
            entry.next_block(),
        );
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// This function traverses the whole path creating anything that does not
    /// yet exist returning the last created entry. This means using parent and
    /// current dir parts in a path that in the end directs to an already
    /// existing path might still create new directories that arent actually being used.
    fn create_entry_at<'p>(
        chain_index: &mut ChainIndex,
        blowfish: Option<&Blowfish>,
        mut stream: &mut B,
        chain: ChainOffset,
        path: &'p str,
    ) -> OpenResult<(ChainOffset, usize, &'p str)> {
        use crate::io::{allocate_empty_block, allocate_new_block_chain, write_chain_entry};
        let (mut current_chain_index, mut components) = chain_index
            .validate_dir_path_until(chain, path)
            .map_err(|e| IoError::new(IoErrorKind::InvalidInput, e))?
            .ok_or_else(|| IoError::from(IoErrorKind::AlreadyExists))?;
        while let Some(component) = components.next() {
            let current_chain = chain_index
                .get_mut(current_chain_index)
                .ok_or_else(|| IoError::from(IoErrorKind::InvalidInput))?;
            let empty_pos = current_chain.entries().position(PackEntry::is_empty);
            let chain_entry_idx = if let Some(idx) = empty_pos {
                idx
            } else {
                // current chain is full so create a new block and append it
                let (offset, block) = allocate_empty_block(blowfish, &mut stream)?;
                let chain_entry_idx = current_chain.num_entries();
                current_chain.push_and_link(offset, block);
                write_chain_entry(blowfish, &mut stream, current_chain, chain_entry_idx - 1)?;
                chain_entry_idx
            };
            // Are we done after this? if not, create a new blockchain since this is a new
            // directory
            if components.peek().is_some() {
                let block_chain = allocate_new_block_chain(
                    blowfish,
                    &mut stream,
                    current_chain,
                    component,
                    chain_entry_idx,
                )?;
                current_chain_index = block_chain.chain_index();
                chain_index.insert(current_chain_index, block_chain);
            } else {
                return Ok((current_chain.chain_index(), chain_entry_idx, component));
            }
        }
        Err(IoErrorKind::AlreadyExists.into())
    }
}

fn check_root(path: &str) -> OpenResult<&str> {
    path.strip_prefix("/").ok_or_else(|| IoError::new(IoErrorKind::InvalidInput, "invalid path"))
}
