use block_modes::BlockMode;

use std::fs::OpenOptions;
use std::path::{Component, Path};

use crate::constants::*;
use crate::error::{Error, Pk2Result};
use crate::fs::{Directory, File, FileMut};
use crate::Blowfish;
use crate::ChainIndex;
use crate::PhysicalFile;

mod block_chain;
mod block_manager;
mod entry;
mod header;

pub(crate) use self::block_chain::{PackBlock, PackBlockChain};
pub(crate) use self::block_manager::BlockManager;
pub(crate) use self::entry::PackEntry;
pub(crate) use self::header::PackHeader;

pub struct Pk2 {
    pub(crate) file: PhysicalFile,
    // we'll make sure to uphold runtime borrow rules, this is needed to allow borrowing file names
    // and such. This will be fine given that blocks can only move in memory through mutating
    // operations on themselves which cannot work if their name or anything similar is
    // borrowed.
    pub(crate) block_manager: BlockManager,
}

impl Pk2 {
    pub fn create<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Pk2Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path.as_ref())?;
        let (header, mut file) = if key.as_ref().is_empty() {
            (PackHeader::default(), PhysicalFile::new(file, None))
        } else {
            let mut bf = create_blowfish(key.as_ref())?;
            (
                PackHeader::new_encrypted(&mut bf),
                PhysicalFile::new(file, Some(bf)),
            )
        };

        header.to_writer(&mut file)?;
        let mut block = PackBlock::new(PK2_ROOT_BLOCK.0);
        block[0] = PackEntry::new_directory(".".to_owned(), PK2_ROOT_BLOCK, None);
        file.write_block(&block)?;

        let block_manager = BlockManager::new(&file)?;
        Ok(Pk2 {
            file,
            block_manager,
        })
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Pk2Result<Self> {
        let mut file = OpenOptions::new().write(true).read(true).open(path)?;
        let header = PackHeader::from_reader(&mut file)?;
        if &header.signature != PK2_SIGNATURE {
            return Err(Error::CorruptedFile);
        }
        if header.version != PK2_VERSION {
            return Err(Error::UnsupportedVersion);
        }

        let bf = if header.encrypted {
            let mut bf = create_blowfish(key.as_ref())?;
            let mut checksum = *PK2_CHECKSUM;
            let _ = bf.encrypt_nopad(&mut checksum);
            if checksum[..PK2_CHECKSUM_STORED] != header.verify[..PK2_CHECKSUM_STORED] {
                return Err(Error::InvalidKey);
            } else {
                Some(bf)
            }
        } else {
            None
        };
        let file = PhysicalFile::new(file, bf);
        let block_manager = BlockManager::new(&file)?;
        Ok(Pk2 {
            file,
            block_manager,
        })
    }

    #[inline(always)]
    pub(crate) fn get_chain(&self, chain: ChainIndex) -> Option<&PackBlockChain> {
        self.block_manager.get(chain)
    }

    #[inline(always)]
    pub(crate) fn get_chain_mut(&mut self, chain: ChainIndex) -> Option<&mut PackBlockChain> {
        self.block_manager.get_mut(chain)
    }

    #[inline(always)]
    pub(crate) fn get_entry(&self, chain: ChainIndex, entry: usize) -> Option<&PackEntry> {
        self.get_chain(chain).and_then(|chain| chain.get(entry))
    }

    #[inline(always)]
    pub(crate) fn get_entry_mut(
        &mut self,
        chain: ChainIndex,
        entry: usize,
    ) -> Option<&mut PackEntry> {
        self.get_chain_mut(chain)
            .and_then(|chain| chain.get_mut(entry))
    }

    fn root_resolve_path_to_entry_and_parent<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Pk2Result<Option<(&PackBlockChain, usize, &PackEntry)>> {
        self.block_manager
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)
    }
}

impl Pk2 {
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

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Pk2Result<File> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or(Error::ExpectedFile))?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        Ok(File::new(self, chain, entry_idx))
    }

    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<FileMut> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or(Error::ExpectedFile))?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        Ok(FileMut::new(self, chain, entry_idx))
    }

    /// Currently only replaces the entry with an empty one making the data
    /// inaccessible by normal means
    pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<()> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or(Error::ExpectedFile))?;
        Self::is_file(entry)?;

        let next_block = entry.next_block();
        let chain_index = chain.chain_index();
        let file_offset = chain.file_offset_for_entry(entry_idx).unwrap();
        self.get_entry_mut(chain_index, entry_idx)
            .map(PackEntry::clear);
        self.file
            .write_entry_at(file_offset, &PackEntry::Empty { next_block })?;
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> Pk2Result<FileMut> {
        use std::ffi::OsStr;
        let path = check_root(path.as_ref())?;
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .map(ToOwned::to_owned)
            .ok_or(Error::NonUnicodePath)?;
        let (chain, entry_idx) = self.create_entry_at(PK2_ROOT_BLOCK, path)?;
        let entry = self.get_entry_mut(chain, entry_idx).unwrap();
        *entry = PackEntry::new_file(file_name, 0, 0, entry.next_block());
        Ok(FileMut::new(self, chain, entry_idx))
    }

    pub fn open_directory<P: AsRef<Path>>(&self, path: P) -> Pk2Result<Directory> {
        let (chain, entry_idx) = match self.root_resolve_path_to_entry_and_parent(path)? {
            Some((chain, entry_idx, entry)) => {
                Self::is_dir(entry)?;
                (chain.chain_index(), entry_idx)
            }
            // path was just root
            None => (PK2_ROOT_BLOCK, 0),
        };
        Ok(Directory::new(self, chain, entry_idx))
    }
}

impl Pk2 {
    /// This function traverses the whole path creating anything that does not
    /// yet exist returning the last created entry. This means using parent and
    /// current dir parts in a path that in the end directs to an already
    /// existing path might still create new directories. TODO: Experiment
    /// with a recursive version to avoid borrowck?
    fn create_entry_at(
        &mut self,
        chain: ChainIndex,
        path: &Path,
    ) -> Pk2Result<(ChainIndex, usize)> {
        let (mut current_chain_index, path) =
            self.block_manager.validate_dir_path_until(chain, path)?;
        let mut components = path.components().peekable();
        while let Some(component) = components.next() {
            match component {
                Component::Normal(p) => {
                    let current_chain = self
                        .block_manager
                        .get_mut(current_chain_index)
                        .ok_or(Error::InvalidChainIndex)?;
                    let chain_entry_idx = match current_chain.find_first_empty_mut() {
                        Some((idx, _)) => idx,
                        None => {
                            // current chain is full so create a new block and append it
                            let new_block_offset = self.file.len()?;
                            let block = PackBlock::new(new_block_offset);
                            self.file.write_block(&block)?;
                            let last_idx = current_chain.entries_mut().count() - 1;
                            current_chain[last_idx].set_next_block(new_block_offset);
                            self.file.write_entry_at(
                                current_chain.file_offset_for_entry(last_idx).unwrap(),
                                &current_chain[last_idx],
                            )?;
                            current_chain.push(block);
                            last_idx + 1
                        }
                    };
                    // Are we done after this? if not, create a new blockchain since this is a new
                    // directory
                    if components.peek().is_some() {
                        let new_chain_offset = self.file.len().map(ChainIndex)?;
                        let entry = &mut current_chain[chain_entry_idx];
                        *entry = PackEntry::new_directory(
                            p.to_str()
                                .map(ToOwned::to_owned)
                                .ok_or(Error::NonUnicodePath)?,
                            new_chain_offset,
                            entry.next_block(),
                        );
                        let offset = current_chain
                            .file_offset_for_entry(chain_entry_idx)
                            .unwrap();
                        let mut block = PackBlock::new(new_chain_offset.0);
                        block[0] =
                            PackEntry::new_directory(".".to_string(), new_chain_offset, None);
                        block[1] = PackEntry::new_directory(
                            "..".to_string(),
                            current_chain.chain_index(),
                            None,
                        );
                        self.file.write_block(&block)?;
                        self.file
                            .write_entry_at(offset, &current_chain[chain_entry_idx])?;
                        self.block_manager
                            .insert(new_chain_offset, PackBlockChain::from_blocks(vec![block]));
                        current_chain_index = new_chain_offset;
                    } else {
                        return Ok((current_chain.chain_index(), chain_entry_idx));
                    }
                }
                Component::CurDir => (),
                Component::ParentDir => {
                    current_chain_index = self
                        .block_manager
                        .get_mut(current_chain_index)
                        .ok_or(Error::InvalidChainIndex)?
                        .entries()
                        .find_map(|entry| match entry {
                            PackEntry::Directory {
                                name, pos_children, ..
                            } if name == ".." => Some(*pos_children),
                            _ => None,
                        })
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
    Blowfish::new_varkey(&key).map_err(|_| Error::InvalidKey)
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
