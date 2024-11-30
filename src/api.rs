pub mod fs;
use self::fs::{Directory, File, FileMut};

use std::marker::PhantomData;
use std::path::{Component, Path};
use std::{fs as stdfs, io};

use crate::blowfish::Blowfish;
use crate::constants::{
    PK2_CHECKSUM, PK2_CURRENT_DIR_IDENT, PK2_PARENT_DIR_IDENT, PK2_ROOT_BLOCK,
    PK2_ROOT_BLOCK_VIRTUAL,
};
use crate::data::block_chain::{PackBlock, PackBlockChain};
use crate::data::block_manager::BlockManager;
use crate::data::entry::PackEntry;
use crate::data::header::PackHeader;
use crate::data::{ChainIndex, StreamOffset};
use crate::error::{ChainLookupError, ChainLookupResult, OpenError, OpenResult};
use crate::io::RawIo;
use crate::{Lock, LockChoice, ReadOnly};

/// A Pk2 archive.
pub struct Pk2<Buffer, L: LockChoice> {
    stream: <L as LockChoice>::Lock<Buffer>,
    blowfish: Option<Box<Blowfish>>,
    block_manager: BlockManager,
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

    /// Opens an archive at the given path with its file index sorted.
    ///
    /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    /// operations on the file making this operation potentially slow.
    pub fn open_sorted<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new().read(true).open(path)?;
        let mut this = Self::_open_in_impl(ReadOnly(file), key)?;
        this.block_manager.sort();
        Ok(this)
    }
}

impl<L: LockChoice> Pk2<io::Cursor<Vec<u8>>, L> {
    /// Creates a new archive in memory.
    pub fn create_new_in_memory<K: AsRef<[u8]>>(
        key: K,
    ) -> Result<Self, crate::blowfish::InvalidKey> {
        Self::_create_impl(io::Cursor::new(Vec::with_capacity(4096)), key).map_err(|e| {
            debug_assert!(matches!(&e, OpenError::InvalidKey));
            // the only error that can actually occur here is an InvalidKey error
            crate::blowfish::InvalidKey
        })
    }
}

impl<L: LockChoice> From<Pk2<io::Cursor<Vec<u8>>, L>> for Vec<u8> {
    fn from(pk2: Pk2<io::Cursor<Vec<u8>>, L>) -> Self {
        pk2.stream.into_inner().into_inner()
    }
}

impl<B, L> Pk2<B, L>
where
    B: io::Read + io::Seek,
    L: LockChoice,
{
    /// Opens an archive from the given stream.
    ///
    /// Note this eagerly parses the whole archive's file table into memory incurring a lot of read
    /// operations on the stream.
    pub fn open_in<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        stream.seek(io::SeekFrom::Start(0))?;
        Self::_open_in_impl(stream, key)
    }

    fn _open_in_impl<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        let header = PackHeader::from_reader(&mut stream)?;
        header.validate_sig()?;
        let blowfish = if header.encrypted {
            let bf = Blowfish::new(key.as_ref())?;
            let mut checksum = *PK2_CHECKSUM;
            bf.encrypt(&mut checksum);
            header.verify(checksum)?;
            Some(Box::new(bf))
        } else {
            None
        };
        let block_manager = BlockManager::new(blowfish.as_deref(), &mut stream)?;

        Ok(Pk2 {
            stream: <L as LockChoice>::Lock::new(stream),
            blowfish,
            block_manager,
            유령: PhantomData,
        })
    }
}

impl<B, L> Pk2<B, L>
where
    B: io::Read + io::Write + io::Seek,
    L: LockChoice,
{
    pub fn create_new_in<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        stream.seek(io::SeekFrom::Start(0))?;
        Self::_create_impl(stream, key)
    }

    fn _create_impl<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        let (header, blowfish) = if key.as_ref().is_empty() {
            (PackHeader::default(), None)
        } else {
            let bf = Blowfish::new(key.as_ref())?;
            (PackHeader::new_encrypted(&bf), Some(Box::new(bf)))
        };

        header.to_writer(&mut stream)?;
        let mut block = PackBlock::default();
        block[0] = PackEntry::new_directory(PK2_CURRENT_DIR_IDENT, PK2_ROOT_BLOCK, None);
        crate::io::write_block(blowfish.as_deref(), &mut stream, PK2_ROOT_BLOCK.into(), &block)?;

        let block_manager = BlockManager::new(blowfish.as_deref(), &mut stream)?;
        Ok(Pk2 { stream: L::new_locked(stream), blowfish, block_manager, 유령: PhantomData })
    }
}

impl<L: LockChoice, B> Pk2<B, L> {
    fn get_chain(&self, chain: ChainIndex) -> Option<&PackBlockChain> {
        self.block_manager.get(chain)
    }

    fn get_chain_mut(&mut self, chain: ChainIndex) -> Option<&mut PackBlockChain> {
        self.block_manager.get_mut(chain)
    }

    fn get_entry(&self, chain: ChainIndex, entry: usize) -> Option<&PackEntry> {
        self.get_chain(chain).and_then(|chain| chain.get(entry))
    }

    fn get_entry_mut(&mut self, chain: ChainIndex, entry: usize) -> Option<&mut PackEntry> {
        self.get_chain_mut(chain).and_then(|chain| chain.get_mut(entry))
    }

    fn root_resolve_path_to_entry_and_parent<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> ChainLookupResult<(ChainIndex, usize, &PackEntry)> {
        self.block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)
    }

    fn is_file(entry: &PackEntry) -> ChainLookupResult<()> {
        match entry.is_file() {
            true => Ok(()),
            false => Err(ChainLookupError::ExpectedFile),
        }
    }

    fn is_dir(entry: &PackEntry) -> ChainLookupResult<()> {
        match entry.is_directory() {
            true => Ok(()),
            false => Err(ChainLookupError::ExpectedDirectory),
        }
    }
}

impl<B, L: LockChoice> Pk2<B, L> {
    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> ChainLookupResult<File<B, L>> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;
        Ok(File::new(self, chain, entry_idx))
    }

    pub fn open_directory<P: AsRef<Path>>(&self, path: P) -> ChainLookupResult<Directory<B, L>> {
        let path = check_root(path.as_ref())?;
        let (chain, entry_idx) =
            match self.block_manager.resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, path) {
                Ok((chain, entry_idx, entry)) => {
                    Self::is_dir(entry)?;
                    (chain, entry_idx)
                }
                // path was just root
                Err(ChainLookupError::InvalidPath) => (PK2_ROOT_BLOCK_VIRTUAL, 0),
                Err(e) => return Err(e),
            };
        Ok(Directory::new(self, chain, entry_idx))
    }

    pub fn open_root_dir(&self) -> Directory<B, L> {
        Directory::new(self, PK2_ROOT_BLOCK_VIRTUAL, 0)
    }

    /// Invokes cb on every file in the sub directories of `base`, including
    /// files inside of its subdirectories. Cb gets invoked with its
    /// relative path to `base` and the file object.
    pub fn for_each_file(
        &self,
        base: impl AsRef<Path>,
        cb: impl FnMut(&Path, File<B, L>) -> io::Result<()>,
    ) -> io::Result<()> {
        self.open_directory(base)?.for_each_file(cb)
    }
}

impl<B, L> Pk2<B, L>
where
    B: io::Read + io::Seek,
    L: LockChoice,
{
    pub fn read<P: AsRef<Path>>(&self, path: P) -> io::Result<Vec<u8>> {
        let mut file = self.open_file(path)?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        std::io::Read::read_to_end(&mut file, &mut buf)?;
        Ok(buf)
    }
}

impl<B, L> Pk2<B, L>
where
    B: io::Read + io::Write + io::Seek,
    L: LockChoice,
{
    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> ChainLookupResult<FileMut<B, L>> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// Currently only replaces the entry with an empty one making the data
    /// inaccessible by normal means
    pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let (chain_index, entry_idx, entry) = self
            .block_manager
            .resolve_path_to_entry_and_parent_mut(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?;
        Self::is_file(entry)?;
        entry.clear();

        self.stream.with_lock(|stream| {
            crate::io::write_chain_entry(
                self.blowfish.as_deref(),
                stream,
                self.get_chain(chain_index).unwrap(),
                entry_idx,
            )
        })?;
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<FileMut<B, L>> {
        let path = check_root(path.as_ref())?;
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(ChainLookupError::InvalidPath)?;
        let (chain, entry_idx) = self.stream.with_lock(|stream| {
            Self::create_entry_at(
                &mut self.block_manager,
                self.blowfish.as_deref(),
                stream,
                PK2_ROOT_BLOCK,
                path,
            )
        })?;
        let entry = self.get_entry_mut(chain, entry_idx).unwrap();
        *entry = PackEntry::new_file(file_name, StreamOffset(0), 0, entry.next_block());
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// This function traverses the whole path creating anything that does not
    /// yet exist returning the last created entry. This means using parent and
    /// current dir parts in a path that in the end directs to an already
    /// existing path might still create new directories that arent actually being used.
    fn create_entry_at(
        block_manager: &mut BlockManager,
        blowfish: Option<&Blowfish>,
        mut stream: &mut B,
        chain: ChainIndex,
        path: &Path,
    ) -> io::Result<(ChainIndex, usize)> {
        use crate::io::{allocate_empty_block, allocate_new_block_chain, write_chain_entry};
        let (mut current_chain_index, mut components) = block_manager
            .validate_dir_path_until(chain, path)?
            .ok_or_else(|| io::Error::from(io::ErrorKind::AlreadyExists))?;
        while let Some(component) = components.next() {
            match component {
                Component::Normal(p) => {
                    let current_chain = block_manager
                        .get_mut(current_chain_index)
                        .ok_or(ChainLookupError::InvalidChainIndex)?;
                    let empty_pos = current_chain.entries().position(PackEntry::is_empty);
                    let chain_entry_idx = if let Some(idx) = empty_pos {
                        idx
                    } else {
                        // current chain is full so create a new block and append it
                        let (offset, block) = allocate_empty_block(blowfish, &mut stream)?;
                        let chain_entry_idx = current_chain.num_entries();
                        current_chain.push_and_link(offset, block);
                        write_chain_entry(
                            blowfish,
                            &mut stream,
                            current_chain,
                            chain_entry_idx - 1,
                        )?;
                        chain_entry_idx
                    };
                    // Are we done after this? if not, create a new blockchain since this is a new
                    // directory
                    if components.peek().is_some() {
                        let dir_name = p.to_str().ok_or(ChainLookupError::InvalidPath)?;
                        let block_chain = allocate_new_block_chain(
                            blowfish,
                            &mut stream,
                            current_chain,
                            dir_name,
                            chain_entry_idx,
                        )?;
                        current_chain_index = block_chain.chain_index();
                        block_manager.insert(current_chain_index, block_chain);
                    } else {
                        return Ok((current_chain.chain_index(), chain_entry_idx));
                    }
                }
                Component::ParentDir => {
                    current_chain_index = block_manager
                        .get_mut(current_chain_index)
                        .ok_or(ChainLookupError::InvalidChainIndex)
                        .and_then(|entry| entry.find_block_chain_index_of(PK2_PARENT_DIR_IDENT))?
                }
                Component::CurDir => (),
                _ => unreachable!(),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }
}

fn check_root(path: &Path) -> ChainLookupResult<&Path> {
    path.strip_prefix("/").map_err(|_| ChainLookupError::InvalidPath)
}

// #[cfg(test)]
// mod test {
//     use std::io;
//     #[test]
//     fn create_already_existing() {
//         let mut archive = super::Pk2::create_new_in_memory("").unwrap();
//         archive.create_file("/test/foo.baz").unwrap();
//         match archive.create_file("/test/foo.baz") {
//             Err(e) => assert_eq!(e.kind(), io::ErrorKind::AlreadyExists),
//             Ok(_) => panic!("file was created twice?"),
//         };
//     }
// }
