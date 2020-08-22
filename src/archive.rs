use std::cell::RefCell;
use std::path::{Component, Path};
use std::{fs as stdfs, io};

use crate::constants::{
    PK2_CHECKSUM, PK2_CURRENT_DIR_IDENT, PK2_PARENT_DIR_IDENT, PK2_ROOT_BLOCK,
    PK2_ROOT_BLOCK_VIRTUAL,
};
use crate::error::{ChainLookupError, ChainLookupResult, OpenError, OpenResult};
use crate::io::RawIo;
use crate::Blowfish;

pub mod fs;
use self::fs::{DirEntry, Directory, File, FileMut};

use crate::raw::block_chain::{PackBlock, PackBlockChain};
use crate::raw::block_manager::BlockManager;
use crate::raw::entry::*;
use crate::raw::header::PackHeader;
use crate::raw::{ChainIndex, StreamOffset};

pub struct Pk2<B = stdfs::File> {
    stream: RefCell<B>,
    blowfish: Option<Blowfish>,
    block_manager: BlockManager,
}

impl Pk2<stdfs::File> {
    pub fn create_new<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path.as_ref())?;
        Self::_create_impl(file, key)
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> OpenResult<Self> {
        let file = stdfs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(path)?;
        Self::_open_in_impl(file, key)
    }
}

impl Pk2<io::Cursor<Vec<u8>>> {
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

impl<B> Pk2<B>
where
    B: io::Read + io::Seek,
{
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
            let _ = bf.encrypt(&mut checksum);
            header.verify(checksum)?;
            Some(bf)
        } else {
            None
        };
        let block_manager = BlockManager::new(blowfish.as_ref(), &mut stream)?;

        Ok(Pk2 {
            stream: RefCell::new(stream),
            blowfish,
            block_manager,
        })
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Write + io::Seek,
{
    pub fn create_new_in<K: AsRef<[u8]>>(mut stream: B, key: K) -> OpenResult<Self> {
        stream.seek(io::SeekFrom::Start(0))?;
        Self::_create_impl(stream, key)
    }

    fn _create_impl<K: AsRef<[u8]>>(stream: B, key: K) -> OpenResult<Self> {
        let (header, mut stream, blowfish) = if key.as_ref().is_empty() {
            (PackHeader::default(), stream, None)
        } else {
            let bf = Blowfish::new(key.as_ref())?;
            (PackHeader::new_encrypted(&bf), stream, Some(bf))
        };

        header.to_writer(&mut stream)?;
        let mut block = PackBlock::default();
        block[0] = PackEntry::new_directory(PK2_CURRENT_DIR_IDENT, PK2_ROOT_BLOCK, None);
        crate::io::write_block(
            blowfish.as_ref(),
            &mut stream,
            PK2_ROOT_BLOCK.into(),
            &block,
        )?;

        let block_manager = BlockManager::new(blowfish.as_ref(), &mut stream)?;
        Ok(Pk2 {
            stream: RefCell::new(stream),
            blowfish,
            block_manager,
        })
    }
}

impl<B> Pk2<B> {
    #[inline(always)]
    fn get_chain(&self, chain: ChainIndex) -> Option<&PackBlockChain> {
        self.block_manager.get(chain)
    }

    #[inline(always)]
    fn get_chain_mut(&mut self, chain: ChainIndex) -> Option<&mut PackBlockChain> {
        self.block_manager.get_mut(chain)
    }

    #[inline(always)]
    fn get_entry(&self, chain: ChainIndex, entry: usize) -> Option<&PackEntry> {
        self.get_chain(chain).and_then(|chain| chain.get(entry))
    }

    #[inline(always)]
    fn get_entry_mut(&mut self, chain: ChainIndex, entry: usize) -> Option<&mut PackEntry> {
        self.get_chain_mut(chain)
            .and_then(|chain| chain.get_mut(entry))
    }

    fn root_resolve_path_to_entry_and_parent<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> ChainLookupResult<(ChainIndex, usize, &PackEntry)> {
        self.block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)
    }

    pub(self) fn is_file(entry: &PackEntry) -> ChainLookupResult<()> {
        match entry.is_file() {
            true => Ok(()),
            false => Err(ChainLookupError::ExpectedFile),
        }
    }

    pub(self) fn is_dir(entry: &PackEntry) -> ChainLookupResult<()> {
        match entry.is_dir() {
            true => Ok(()),
            false => Err(ChainLookupError::ExpectedDirectory),
        }
    }
}

impl<B> Pk2<B> {
    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> ChainLookupResult<File<B>> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;
        Ok(File::new(self, chain, entry_idx))
    }

    pub fn open_directory<P: AsRef<Path>>(&self, path: P) -> ChainLookupResult<Directory<B>> {
        let path = check_root(path.as_ref())?;
        let (chain, entry_idx) = match self
            .block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, path)
        {
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

    /// Invokes cb on every file in the sub directories of `base`, including
    /// files inside of its subdirectories. Cb gets invoked with its
    /// relative path to `base` and the file object.
    // Todo, replace this with a file_paths iterator once generators are stable
    pub fn for_each_file(
        &self,
        base: impl AsRef<Path>,
        mut cb: impl FnMut(&Path, File<B>) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut path = std::path::PathBuf::new();
        let mut stack = vec![self.open_directory(base)?];
        let mut first_iteration = true;
        while let Some(dir) = stack.pop() {
            if !first_iteration {
                path.push(dir.name());
            } else {
                first_iteration = false;
            };
            let mut files_only = true;
            for entry in dir.entries() {
                match entry {
                    DirEntry::Directory(dir) => {
                        stack.push(dir);
                        files_only = false;
                    }
                    DirEntry::File(file) => {
                        path.push(file.name());
                        cb(&path, file)?;
                        path.pop();
                    }
                }
            }
            if files_only {
                path.pop();
            }
        }
        Ok(())
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Seek,
{
    pub fn read<P: AsRef<Path>>(&self, path: P) -> io::Result<Vec<u8>> {
        let mut file = self.open_file(path)?;
        let mut buf = vec![0; file.size() as usize];
        std::io::Read::read_to_end(&mut file, &mut buf)?;
        Ok(buf)
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Write + io::Seek,
{
    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> ChainLookupResult<FileMut<B>> {
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

        crate::io::write_chain_entry(
            self.blowfish.as_ref(),
            &mut *self.stream.borrow_mut(),
            self.get_chain(chain_index).unwrap(),
            entry_idx,
        )?;
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<FileMut<B>> {
        let path = check_root(path.as_ref())?;
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(ChainLookupError::InvalidPath)?;
        let (chain, entry_idx) = Self::create_entry_at(
            &mut self.block_manager,
            self.blowfish.as_ref(),
            &mut *self.stream.borrow_mut(),
            PK2_ROOT_BLOCK,
            path,
        )?;
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
                            &current_chain,
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

#[inline]
fn check_root(path: &Path) -> ChainLookupResult<&Path> {
    path.strip_prefix("/")
        .map_err(|_| ChainLookupError::InvalidPath)
}

#[cfg(test)]
mod test {
    use std::io;
    #[test]
    fn create_already_existing() {
        let mut archive = super::Pk2::create_new_in_memory("").unwrap();
        archive.create_file("/test/foo.baz").unwrap();
        match archive.create_file("/test/foo.baz") {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::AlreadyExists),
            Ok(_) => panic!("file was created twice?"),
        };
    }
}
