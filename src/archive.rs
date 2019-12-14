use std::cell::RefCell;
use std::path::{Component, Path};
use std::{fs as stdfs, io};

use crate::constants::{PK2_CHECKSUM, PK2_CURRENT_DIR_IDENT, PK2_ROOT_BLOCK, PK2_SALT};
use crate::error::{Error, Pk2Result};
use crate::io::RawIo;
use crate::Blowfish;

pub mod fs;
use self::fs::{Directory, File, FileMut};

use crate::raw::block_chain::{PackBlock, PackBlockChain};
use crate::raw::block_manager::BlockManager;
use crate::raw::entry::*;
use crate::raw::header::PackHeader;
use crate::raw::ChainIndex;

pub struct Pk2<B = stdfs::File> {
    file: RefCell<B>,
    blowfish: Option<Blowfish>,
    block_manager: BlockManager,
}

impl Pk2<stdfs::File> {
    pub fn create_new<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Pk2Result<Self> {
        let file = stdfs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path.as_ref())?;
        Self::_create_impl(file, key)
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Pk2Result<Self> {
        let file = stdfs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(path)?;
        Self::_open_in_impl(file, key)
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Seek,
{
    pub fn open_in<K: AsRef<[u8]>>(mut file: B, key: K) -> Pk2Result<Self> {
        file.seek(io::SeekFrom::Start(0))?;
        Self::_open_in_impl(file, key)
    }

    fn _open_in_impl<K: AsRef<[u8]>>(mut file: B, key: K) -> Pk2Result<Self> {
        let header = PackHeader::from_reader(&mut file)?;
        header.validate_sig()?;
        let blowfish = if header.encrypted {
            let bf = create_blowfish(key.as_ref())?;
            let mut checksum = *PK2_CHECKSUM;
            let _ = bf.encrypt(&mut checksum);
            header.verify(checksum)?;
            Some(bf)
        } else {
            None
        };
        let block_manager = BlockManager::new(blowfish.as_ref(), &mut file)?;

        Ok(Pk2 {
            file: RefCell::new(file),
            blowfish,
            block_manager,
        })
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Write + io::Seek,
{
    pub fn create_new_in<K: AsRef<[u8]>>(mut file: B, key: K) -> Pk2Result<Self> {
        file.seek(io::SeekFrom::Start(0))?;
        Self::_create_impl(file, key)
    }

    fn _create_impl<K: AsRef<[u8]>>(file: B, key: K) -> Pk2Result<Self> {
        let (header, mut file, blowfish) = if key.as_ref().is_empty() {
            (PackHeader::default(), file, None)
        } else {
            let bf = create_blowfish(key.as_ref())?;
            (PackHeader::new_encrypted(&bf), file, Some(bf))
        };

        header.to_writer(&mut file)?;
        let mut block = PackBlock::default();
        block[0] = PackEntry::new_directory(PK2_CURRENT_DIR_IDENT, PK2_ROOT_BLOCK, None);
        crate::io::write_block(blowfish.as_ref(), &mut file, PK2_ROOT_BLOCK.into(), &block)?;

        let block_manager = BlockManager::new(blowfish.as_ref(), &mut file)?;
        Ok(Pk2 {
            file: RefCell::new(file),
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
    ) -> Pk2Result<(&PackBlockChain, usize, &PackEntry)> {
        self.block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)
    }

    fn is_file(entry: &PackEntry) -> Pk2Result<()> {
        match entry.is_file() {
            true => Ok(()),
            false => Err(Error::ExpectedFile),
        }
    }

    fn is_dir(entry: &PackEntry) -> Pk2Result<()> {
        match entry.is_dir() {
            true => Ok(()),
            false => Err(Error::ExpectedDirectory),
        }
    }
}

impl<B> Pk2<B> {
    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Pk2Result<File<B>> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        Ok(File::new(self, chain, entry_idx))
    }

    pub fn open_directory<P: AsRef<Path>>(&self, path: P) -> Pk2Result<Directory<B>> {
        let path = check_root(path.as_ref())?;
        let (chain, entry_idx) = match self
            .block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, path)
        {
            Ok((chain, entry_idx, entry)) => {
                Self::is_dir(entry)?;
                (chain.chain_index(), entry_idx)
            }
            // path was just root
            Err(Error::InvalidPath) => (PK2_ROOT_BLOCK, 0),
            Err(e) => return Err(e),
        };
        Ok(Directory::new(self, chain, entry_idx))
    }
}

impl<B> Pk2<B>
where
    B: io::Read + io::Write + io::Seek,
{
    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<FileMut<B>> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// Currently only replaces the entry with an empty one making the data
    /// inaccessible by normal means
    pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<()> {
        let (chain, entry_idx, entry) = self.root_resolve_path_to_entry_and_parent(path)?;
        Self::is_file(entry)?;

        let next_block = entry.next_block();
        let chain_index = chain.chain_index();
        let file_offset = chain.file_offset_for_entry(entry_idx).unwrap();
        self.get_entry_mut(chain_index, entry_idx)
            .map(PackEntry::clear);

        crate::io::write_entry_at(
            self.blowfish.as_ref(),
            &mut *self.file.borrow_mut(),
            file_offset,
            &PackEntry::new_empty(next_block),
        )?;
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<FileMut<B>> {
        use std::ffi::OsStr;
        let path = check_root(path.as_ref())?;
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .map(ToOwned::to_owned)
            .ok_or(Error::NonUnicodePath)?;
        let (chain, entry_idx) = Self::create_entry_at(
            &mut self.block_manager,
            self.blowfish.as_ref(),
            &mut *self.file.borrow_mut(),
            PK2_ROOT_BLOCK,
            path,
        )?;
        let entry = self.get_entry_mut(chain, entry_idx).unwrap();
        *entry = PackEntry::new_file(file_name, 0, 0, entry.next_block());
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// This function traverses the whole path creating anything that does not
    /// yet exist returning the last created entry. This means using parent and
    /// current dir parts in a path that in the end directs to an already
    /// existing path might still create new directories.
    fn create_entry_at(
        block_manager: &mut BlockManager,
        blowfish: Option<&Blowfish>,
        mut file: &mut B,
        chain: ChainIndex,
        path: &Path,
    ) -> Pk2Result<(ChainIndex, usize)> {
        let (mut current_chain_index, mut components) =
            block_manager.validate_dir_path_until(chain, path)?;
        while let Some(component) = components.next() {
            match component {
                Component::Normal(p) => {
                    let current_chain = block_manager
                        .get_mut(current_chain_index)
                        .ok_or(Error::InvalidChainIndex)?;
                    let empty_pos = current_chain.entries().position(PackEntry::is_empty);
                    let chain_entry_idx = if let Some(idx) = empty_pos {
                        idx
                    } else {
                        // current chain is full so create a new block and append it
                        current_chain.create_new_block(blowfish, &mut file)?
                    };
                    // Are we done after this? if not, create a new blockchain since this is a new
                    // directory
                    if components.peek().is_some() {
                        let dir_name = p.to_str().ok_or(Error::NonUnicodePath)?;
                        let block_chain = crate::io::create_new_block_chain(
                            blowfish,
                            &mut file,
                            current_chain,
                            dir_name,
                            chain_entry_idx,
                        )?;
                        let new_chain_offset = block_chain.chain_index();
                        block_manager.insert(new_chain_offset, block_chain);
                        current_chain_index = new_chain_offset;
                    } else {
                        return Ok((current_chain.chain_index(), chain_entry_idx));
                    }
                }
                Component::CurDir => (),
                Component::ParentDir => {
                    current_chain_index = block_manager
                        .get_mut(current_chain_index)
                        .ok_or(Error::InvalidChainIndex)?
                        .entries()
                        .filter_map(PackEntry::as_directory)
                        .find(|dir| dir.is_parent_link())
                        .map(DirectoryEntry::children_position)
                        .ok_or(Error::InvalidPath)?;
                }
                _ => unreachable!(),
            }
        }
        Err(Error::AlreadyExists)
    }
}

fn create_blowfish(key: &[u8]) -> Pk2Result<Blowfish> {
    let mut key = key.to_vec();
    gen_final_blowfish_key_inplace(&mut key);
    Blowfish::new_varkey(&key)
}

fn gen_final_blowfish_key_inplace(key: &mut [u8]) {
    let key_len = key.len().min(56);

    let mut base_key = [0; 56];
    base_key[0..PK2_SALT.len()].copy_from_slice(&PK2_SALT);

    for i in 0..key_len {
        key[i] ^= base_key[i];
    }
}

#[inline]
fn check_root(path: &Path) -> Pk2Result<&Path> {
    path.strip_prefix("/").map_err(|_| Error::InvalidPath)
}
