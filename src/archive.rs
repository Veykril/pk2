use block_modes::BlockMode;

use std::{
    fs::OpenOptions,
    io::{self, Read, Result},
    path::{Component, Path},
};

use crate::constants::*;
use crate::fs::{Directory, File, FileMut};
use crate::Blowfish;

mod block_chain;
mod block_manager;
mod entry;
mod header;
mod phys_file;

pub(in crate) use self::block_chain::{PackBlock, PackBlockChain};
pub(in crate) use self::block_manager::BlockManager;
pub(in crate) use self::entry::PackEntry;
pub(in crate) use self::header::PackHeader;
pub(in crate) use self::phys_file::PhysFile;

pub struct Pk2 {
    header: PackHeader,
    pub(in crate) file: PhysFile,
    pub(in crate) block_mgr: BlockManager,
}

impl Pk2 {
    pub fn create<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path.as_ref())?;
        let mut bf = Blowfish::new_varkey(&gen_final_blowfish_key(key.as_ref())).unwrap();
        let header = PackHeader::new_encrypted(&mut bf);
        let mut file = PhysFile::new(file, bf);

        header.to_writer(&mut *file.borrow_mut())?;
        let mut block = PackBlock::default();
        block.offset = PK2_ROOT_BLOCK;
        block[0] = PackEntry::new_directory(".".to_owned(), PK2_ROOT_BLOCK, None);
        file.write_block(&block)?;

        let block_mgr = BlockManager::new(&mut file)?;
        Ok(Pk2 {
            header,
            file,
            block_mgr,
        })
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
        let mut file = OpenOptions::new().write(true).read(true).open(path)?;
        let header = PackHeader::from_reader(&mut file)?;
        if &header.signature != PK2_SIGNATURE {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid signature: {:?}", header.signature),
            ))?;
        }
        if header.version != PK2_VERSION {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid file version, require {}", PK2_VERSION),
            ))?;
        }
        let mut bf = Blowfish::new_varkey(&gen_final_blowfish_key(key.as_ref())).unwrap();
        if header.encrypted {
            let mut checksum = *PK2_CHECKSUM;
            let _ = bf.encrypt_nopad(&mut checksum);
            if checksum[..PK2_CHECKSUM_STORED] != header.verify[..PK2_CHECKSUM_STORED] {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid blowfish key",
                ))?;
            }
        }

        let mut file = PhysFile::new(file, bf);
        let block_mgr = BlockManager::new(&mut file)?;
        Ok(Pk2 {
            header,
            file,
            block_mgr,
        })
    }

    pub fn header(&self) -> &PackHeader {
        &self.header
    }
}

impl Pk2 {
    // Every line of this function deserves a comment cause it's a mess
    // This function does not write the entry into the file, it might write needed blocks or entries for the path still
    // It only makes sure to return a blockchain index and an unused entry index in said blockchain
    fn create_entry_at(&mut self, mut chain: u64, path: &Path) -> Result<(u64, usize)> {
        let mut components = path.components().peekable();
        while let Some(component) = components.next() {
            if let Component::Normal(p) = component {
                let block_chain = self.block_mgr.chains.get_mut(&chain).unwrap();
                let idx = if let Some((idx, entry)) = block_chain.find_first_empty_mut() {
                    if components.peek().is_some() {
                        //allocate new blockchain
                        let file_len = self.file.len()?;
                        *entry = PackEntry::new_directory(
                            p.to_str().unwrap().to_owned(),
                            file_len,
                            entry.next_chain(),
                        );
                        let offset = block_chain.get_file_offset_for_entry(idx).unwrap();
                        self.file.write_entry_at(offset, &block_chain[idx])?;
                        let mut block = PackBlock::default();
                        block.offset = offset;
                        block[0] = PackEntry::new_directory(".".to_owned(), offset, None);
                        block[1] =
                            PackEntry::new_directory("..".to_owned(), block_chain.offset(), None);
                        self.file.write_block(&block)?;
                        self.block_mgr
                            .chains
                            .insert(file_len, PackBlockChain::new(vec![block]));
                        chain = file_len;
                        continue;
                    } else {
                        idx
                    }
                } else {
                    let offset = self.file.len()?;
                    let block = self.file.create_new_block_at(offset)?;
                    block_chain.as_mut().last_mut().unwrap()[PK2_FILE_BLOCK_ENTRY_COUNT - 1]
                        .set_next_chain(offset);
                    block_chain.push(block);
                    (block_chain.as_ref().len() - 1) * PK2_FILE_BLOCK_ENTRY_COUNT
                };
                return Ok((block_chain.offset(), idx));
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{:?} unexpected", component),
                ));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Path already exists",
        ))
    }
}

impl Pk2 {
    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        let (chain, path) = self
            .block_mgr
            .validate_dir_path_until(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?;
        let (chain, entry_idx) = self.create_entry_at(chain, path)?;
        let entry = &mut self.block_mgr.chains.get_mut(&chain).unwrap()[entry_idx];
        *entry = PackEntry::new_file(
            path.file_name().unwrap().to_str().unwrap().to_owned(),
            0,
            0,
            entry.next_chain(),
        );
        Ok(FileMut::new(self, chain, entry_idx, Vec::new()))
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let (_, _, entry) = self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
            .unwrap();
        Ok(File::new(self, entry))
    }

    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        let (chain, idx, entry) = self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
            .unwrap();
        let mut buf = Vec::new();
        File::new(self, entry).read_to_end(&mut buf)?;
        let offset = chain.offset();
        Ok(FileMut::new(self, offset, idx, buf))
    }

    /// Currently only replaces the entry with an empty one making the data inaccessible by normal means
    pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let (chain, idx, entry) = self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
            .unwrap();
        let next_chain = entry.next_chain();
        let offset = chain.offset();
        self.block_mgr.chains.get_mut(&offset).unwrap()[idx] = PackEntry::Empty { next_chain };
        Ok(())
    }

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        let (chain, entry) = match self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
        {
            Some((_, _, entry)) => (entry.pos_children().unwrap(), Some(entry)),
            None => (PK2_ROOT_BLOCK, None),
        };
        Ok(Directory::new(self, &self.block_mgr.chains[&chain], entry))
    }
}

fn gen_final_blowfish_key(key: &[u8]) -> Vec<u8> {
    let key_len = key.len().min(56);

    let mut base_key = [0; 56];
    base_key[0..PK2_SALT.len()].copy_from_slice(&PK2_SALT);

    let mut blowfish_key = vec![0; key_len];
    for i in 0..key_len {
        blowfish_key[i] = key[i] ^ base_key[i];
    }
    blowfish_key
}

fn err_not_found(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, msg)
}

fn check_root(path: &Path) -> Result<&Path> {
    path.strip_prefix("/")
        .map_err(|_| err_not_found("Absolute path expected".to_owned()))
}
