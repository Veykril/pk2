use block_modes::BlockMode;

use std::cell::{RefCell, UnsafeCell};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Result};
use std::path::{Component, Path};

use crate::constants::*;
use crate::error::*;
use crate::fs::{Directory, File};
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

// !0 means borrowed mutably
#[derive(PartialEq)]
pub(crate) struct BorrowFlags(u32);

impl BorrowFlags {
    const MUT_FLAG: u32 = !0;
    fn new() -> Self {
        BorrowFlags(0)
    }

    pub(crate) fn try_borrow(&mut self) -> Result<()> {
        match self.0.saturating_add(1) {
            Self::MUT_FLAG => Err(io::Error::new(
                io::ErrorKind::Other,
                "file has already been opened with write access",
            )),
            new_flag => {
                self.0 = new_flag;
                Ok(())
            }
        }
    }

    pub(crate) fn try_borrow_mut(&mut self) -> Result<()> {
        match self.0 {
            0 => {
                self.0 = Self::MUT_FLAG;
                Ok(())
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "file has been already opened with write or read access",
            )),
        }
    }

    pub(crate) fn drop_borrow(&mut self) -> bool {
        match self.0 {
            Self::MUT_FLAG => self.0 = 0,
            _ => self.0 = self.0.saturating_sub(1),
        }
        self.0 == 0
    }
}

pub struct Pk2 {
    header: PackHeader,
    pub(crate) file: PhysicalFile,
    // we'll make sure to uphold runtime borrow rules, this is needed to allow borrowing file names
    // and such. This will be fine given that blocks can only move in memory through mutating
    // operations on themselves which cannot work if their name or anything similar is
    // borrowed.
    pub(crate) block_mgr: UnsafeCell<BlockManager>,
    borrow_map: RefCell<HashMap<(ChainIndex, usize), BorrowFlags>>,
}

impl Pk2 {
    pub fn create<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
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
        let mut block = PackBlock::new();
        block.offset = PK2_ROOT_BLOCK.0;
        block[0] = PackEntry::new_directory(".".to_owned(), PK2_ROOT_BLOCK, None);
        file.write_block(&block)?;

        let block_mgr = UnsafeCell::new(BlockManager::new(&file)?);
        Ok(Pk2 {
            header,
            file,
            block_mgr,
            borrow_map: RefCell::new(HashMap::new()),
        })
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
        let mut file = OpenOptions::new().write(true).read(true).open(path)?;
        let header = PackHeader::from_reader(&mut file)?;
        if &header.signature != PK2_SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid pak signature: {:?}", header.signature),
            ));
        }
        if header.version != PK2_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid file version: {:X}", header.version),
            ));
        }

        let bf = if header.encrypted {
            let mut bf = create_blowfish(key.as_ref())?;
            let mut checksum = *PK2_CHECKSUM;
            let _ = bf.encrypt_nopad(&mut checksum);
            if checksum[..PK2_CHECKSUM_STORED] != header.verify[..PK2_CHECKSUM_STORED] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid blowfish key",
                ));
            } else {
                Some(bf)
            }
        } else {
            None
        };
        let file = PhysicalFile::new(file, bf);
        let block_mgr = UnsafeCell::new(BlockManager::new(&file)?);
        Ok(Pk2 {
            header,
            file,
            block_mgr,
            borrow_map: RefCell::new(HashMap::new()),
        })
    }

    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    #[inline]
    pub(crate) fn get_chain(&self, chain: ChainIndex) -> Option<&PackBlockChain> {
        unsafe { &*self.block_mgr.get() }.get(chain)
    }

    #[inline]
    pub(crate) fn get_chain_mut(&self, chain: ChainIndex) -> Option<&mut PackBlockChain> {
        unsafe { &mut *self.block_mgr.get() }.get_mut(chain)
    }

    pub(crate) fn get_entry(&self, chain: ChainIndex, entry: usize) -> Option<&PackEntry> {
        self.get_chain(chain).and_then(|chain| chain.get(entry))
    }

    pub(crate) fn get_entry_mut(&self, chain: ChainIndex, entry: usize) -> Option<&mut PackEntry> {
        self.get_chain_mut(chain)
            .and_then(|chain| chain.get_mut(entry))
    }

    fn root_resolve_path_to_entry_and_parent<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<Option<(&PackBlockChain, usize, &PackEntry)>> {
        unsafe { &mut *self.block_mgr.get() }
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)
    }

    pub(crate) fn borrow_file(&self, chain: ChainIndex, entry: usize) -> Result<()> {
        self.borrow_map
            .borrow_mut()
            .entry((chain, entry))
            .or_insert_with(BorrowFlags::new)
            .try_borrow()
    }

    pub(crate) fn borrow_file_mut(&self, chain: ChainIndex, entry: usize) -> Result<()> {
        self.borrow_map
            .borrow_mut()
            .entry((chain, entry))
            .or_insert_with(BorrowFlags::new)
            .try_borrow_mut()
    }

    pub(crate) fn drop_borrow(&self, chain: ChainIndex, entry: usize) {
        let mut map = self.borrow_map.borrow_mut();
        if map
            .get_mut(&(chain, entry))
            .map(BorrowFlags::drop_borrow)
            .unwrap_or(false)
        {
            map.remove(&(chain, entry));
        }
    }
}

impl Pk2 {
    fn is_file(entry: &PackEntry) -> Result<()> {
        match entry.is_file() {
            true => Ok(()),
            false => Err(err_dir_found_exp_file()),
        }
    }

    fn is_dir(entry: &PackEntry) -> Result<()> {
        match entry.is_dir() {
            true => Ok(()),
            false => Err(err_file_found_exp_dir()),
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or_else(err_dir_found_exp_file))?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        File::new_read(self, chain, entry_idx)
    }

    pub fn open_file_mut<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or_else(err_dir_found_exp_file))?;
        Self::is_file(entry)?;
        let chain = chain.chain_index();
        File::new_write(self, chain, entry_idx)
    }

    /// Currently only replaces the entry with an empty one making the data
    /// inaccessible by normal means
    pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let (chain, entry_idx, entry) = self
            .root_resolve_path_to_entry_and_parent(path)
            .and_then(|opt| opt.ok_or_else(err_dir_found_exp_file))?;
        Self::is_file(entry)?;
        let next_block = entry.next_block();
        let entry = self.get_entry_mut(chain.chain_index(), entry_idx).unwrap();
        self.file
            .write_entry_at(chain.file_offset_for_entry(entry_idx).unwrap(), entry)?;
        *entry = PackEntry::Empty { next_block };
        Ok(())
    }

    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> Result<File> {
        use std::ffi::OsStr;
        let path = check_root(path.as_ref())?;
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .map(ToOwned::to_owned)
            .ok_or_else(err_path_non_unicode)?;
        let (chain, entry_idx) = self.create_entry_at(PK2_ROOT_BLOCK, path)?;
        let entry = self.get_entry_mut(chain, entry_idx).unwrap();
        *entry = PackEntry::new_file(file_name, 0, 0, entry.next_block());
        File::new_write(self, chain, entry_idx)
    }

    pub fn open_directory<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
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
    /// yet exist returning last the created entry. This means using parent and
    /// current dir parts in a path that in the end directs to an already
    /// existing path might still create new directories. TODO: Experiment
    /// with a recursive version to avoid borrowck?
    fn create_entry_at(&mut self, chain: ChainIndex, path: &Path) -> Result<(ChainIndex, usize)> {
        let block_manager = unsafe { &mut *self.block_mgr.get() };
        let (mut current_chain_index, path) = block_manager.validate_dir_path_until(chain, path)?;
        let mut components = path.components().peekable();
        while let Some(component) = components.next() {
            match component {
                Component::Normal(p) => {
                    let current_chain = block_manager
                        .chains
                        .get_mut(&current_chain_index)
                        .ok_or_else(err_invalid_chain)?;
                    let idx = match current_chain.find_first_empty_mut() {
                        Some((idx, _)) => idx,
                        None => {
                            // current chain is full so create a new block and append it
                            let new_block_offset = self.file.len()?;
                            let mut block = PackBlock::default();
                            block.offset = new_block_offset;
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
                    // Are we done after this? if not, create a new blockchain
                    if components.peek().is_some() {
                        let new_chain_offset = self.file.len().map(ChainIndex)?;
                        let entry = &mut current_chain[idx];
                        *entry = PackEntry::new_directory(
                            p.to_str()
                                .map(ToOwned::to_owned)
                                .ok_or_else(err_path_non_unicode)?,
                            new_chain_offset,
                            entry.next_block(),
                        );
                        let offset = current_chain.file_offset_for_entry(idx).unwrap();
                        let mut block = PackBlock::default();
                        block.offset = new_chain_offset.0;
                        block[0] =
                            PackEntry::new_directory(".".to_string(), new_chain_offset, None);
                        block[1] = PackEntry::new_directory(
                            "..".to_string(),
                            current_chain.chain_index(),
                            None,
                        );
                        self.file.write_block(&block)?;
                        self.file.write_entry_at(offset, &current_chain[idx])?;
                        block_manager.chains.insert(
                            new_chain_offset,
                            PackBlockChain::from_blocks(vec![Box::new(block)]),
                        );
                        current_chain_index = new_chain_offset;
                    } else {
                        return Ok((current_chain.chain_index(), idx));
                    }
                }
                Component::CurDir => unimplemented!(),
                Component::ParentDir => unimplemented!(),
                _ => unreachable!(),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "path already exists",
        ))
    }
}

fn create_blowfish(key: &[u8]) -> Result<Blowfish> {
    let mut key = key.to_vec();
    gen_final_blowfish_key_inplace(&mut key);
    Blowfish::new_varkey(&key)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key length"))
}

fn gen_final_blowfish_key_inplace(key: &mut [u8]) {
    let key_len = key.len().min(56);

    let mut base_key = [0; 56];
    base_key[0..PK2_SALT.len()].copy_from_slice(&PK2_SALT);

    for i in 0..key_len {
        key[i] ^= base_key[i];
    }
}

fn check_root(path: &Path) -> Result<&Path> {
    path.strip_prefix("/")
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "absolute path expected"))
}
