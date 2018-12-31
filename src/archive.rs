use block_modes::BlockMode;
use hashbrown::HashMap;

use std::cell::RefCell;
use std::fs::File as StdFile;
use std::io::{self, Cursor, Read, Result, Seek, SeekFrom, Write};
use std::iter::Peekable;
use std::path::Components;
use std::path::{Component, Path};

use crate::constants::*;
use crate::fs::{Directory, File};
use crate::Blowfish;

mod block_chain;
mod block_manager;
mod entry;
mod header;

pub(crate) use self::block_chain::{PackBlock, PackBlockChain};
pub(crate) use self::block_manager::BlockManager;
pub(crate) use self::entry::PackEntry;
pub(crate) use self::header::PackHeader;
use crate::PackIndex;

pub struct Pk2 {
    header: PackHeader,
    bf: Blowfish,
    pub(crate) file: RefCell<StdFile>,
    pub(crate) block_mgr: BlockManager,
}

impl Pk2 {
    pub fn create<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
        let mut file = StdFile::create(path)?;
        let mut bf = Blowfish::new_varkey(&gen_final_blowfish_key(key.as_ref())).unwrap();
        let header = PackHeader::new_encrypted(&mut bf);

        header.to_writer(&mut file)?;
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        PackBlock {
            offset: PK2_ROOT_BLOCK,
            entries: Default::default(),
        }
        .to_writer(&mut Cursor::new(&mut buf[..]))?;
        let _ = bf.encrypt_nopad(&mut buf);
        file.write_all(&buf)?;

        let block_mgr = BlockManager::new(&mut bf, &mut file)?;
        Ok(Pk2 {
            header,
            bf,
            file: RefCell::new(file),
            block_mgr,
        })
    }

    pub fn open<P: AsRef<Path>, K: AsRef<[u8]>>(path: P, key: K) -> Result<Self> {
        let mut file = StdFile::open(path)?;
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

        let block_mgr = BlockManager::new(&mut bf, &mut file)?;
        Ok(Pk2 {
            header,
            bf,
            file: RefCell::new(file),
            block_mgr,
        })
    }

    pub fn header(&self) -> &PackHeader {
        &self.header
    }
}

impl Pk2 {
    fn create_entry_at(&mut self, chain: u64, path: &Path) -> Result<(u64, usize)> {
        let (chain, mut components) = self.block_mgr.validate_dir_path_until(chain, path)?;
        println!("{:?}", components.clone().collect::<Vec<_>>());
        while let Some(component) = components.next() {
            let name = component.as_os_str().to_str().unwrap();
            if let Component::Normal(p) = component {
                let block_chain = self.block_mgr.chains.get_mut(&chain).unwrap();
                if let Some((idx, entry)) = block_chain.find_first_empty_mut() {
                    if let Some(c) = components.peek() {
                        let next_block_offset = self.file.borrow().metadata()?.len();
                        *entry = PackEntry::new_folder(
                            c.as_os_str().to_str().unwrap().to_owned(),
                            next_block_offset,
                            entry.next_chain(),
                        );
                        let offset = block_chain.get_file_offset_for_entry(idx).unwrap();
                        Self::write_entry_to_file_at(
                            &mut *self.file.borrow_mut(),
                            &mut self.bf,
                            offset,
                            &block_chain[idx],
                        )?;
                        let block = Self::create_new_block_in_file_at(
                            &mut *self.file.borrow_mut(),
                            &mut self.bf,
                            next_block_offset,
                        )?;
                        self.block_mgr
                            .chains
                            .insert(next_block_offset, PackBlockChain::new(vec![block]));
                    } else {
                        return Ok((block_chain.offset(), idx));
                    }
                } else {
                    let next_block_offset = self.file.borrow().metadata()?.len();
                    let block = Self::create_new_block_in_file_at(
                        &mut *self.file.borrow_mut(),
                        &mut self.bf,
                        next_block_offset,
                    )?;
                    block_chain.blocks.last_mut().unwrap()[PK2_FILE_BLOCK_ENTRY_COUNT - 1]
                        .set_next_chain(next_block_offset);
                    block_chain.blocks.push(block);
                }
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

    fn create_new_block_in_file_at<W: Write + Seek>(
        mut file: W,
        bf: &mut Blowfish,
        offset: u64,
    ) -> Result<PackBlock> {
        let mut block = PackBlock::default();
        block.offset = offset;
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        block.to_writer(Cursor::new(&mut buf[..]))?;
        let _ = bf.encrypt_nopad(&mut buf);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buf)?;
        Ok(block)
    }

    fn write_entry_to_file_at<W: Write + Seek>(
        mut file: W,
        bf: &mut Blowfish,
        offset: u64,
        entry: &PackEntry,
    ) -> Result<()> {
        let mut buf = [0; PK2_FILE_ENTRY_SIZE];
        entry.to_writer(&mut buf[..])?;
        let _ = bf.encrypt_nopad(&mut buf);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buf)
    }
}

impl Pk2 {
    pub fn create_file<P: AsRef<Path>>(&mut self, path: P, buf: &[u8]) -> Result<File> {
        let path = check_root(path.as_ref())?;
        let (chain_id, entry_idx) = self.create_entry_at(PK2_ROOT_BLOCK, path.parent().unwrap())?;
        let chain = self.block_mgr.chains.get_mut(&chain_id).unwrap();
        let entry = &mut chain[entry_idx];
        let pos_data = self.file.borrow_mut().seek(SeekFrom::End(0))?;
        assert!(buf.len() < !0u32 as usize);
        self.file.borrow_mut().write_all(buf)?;
        *entry = PackEntry::new_file(path.file_name().unwrap().to_str().unwrap().to_owned(), pos_data, buf.len() as u32, entry.next_chain());
        Ok(File::new(self, (chain_id, entry_idx)))
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let (parent, _) = self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
            .unwrap();
        Ok(File::new(self, parent))
    }

    /*
        pub fn delete_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
            unimplemented!()
        }

        pub fn create_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<DirMut> {
            unimplemented!()
        }
    */

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        let (chain, parent) = match self
            .block_mgr
            .resolve_path_to_entry_and_parent(PK2_ROOT_BLOCK, check_root(path.as_ref())?)?
        {
            Some((parent, entry)) => (entry.pos_children().unwrap(), Some(parent)),
            None => (PK2_ROOT_BLOCK, None),
        };
        Ok(Directory::new(self, chain, parent))
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
