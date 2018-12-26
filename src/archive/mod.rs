use block_modes::BlockMode;
use hashbrown::HashMap;

use std::cell::RefCell;
use std::fs::File as StdFile;
use std::io::{self, Cursor, Read, Result, Seek, SeekFrom, Write};
use std::path::{Component, Path};

use crate::constants::*;
use crate::fs::{Directory, File, FileMut};
use crate::Blowfish;

//pub mod pack_block;
mod block_chain;
mod entry;
mod header;

//use self::pack_block::PackBlock;
pub(crate) use self::block_chain::{PackBlock, PackBlockChain};
pub(crate) use self::entry::PackEntry;
pub(crate) use self::header::PackHeader;

pub struct Archive {
    header: PackHeader,
    bf: Blowfish,
    pub file: RefCell<StdFile>,
    pub blockchains: HashMap<u64, PackBlockChain>,
}

impl Archive {
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

        let mut this = Archive {
            header,
            bf,
            file: RefCell::new(file),
            blockchains: HashMap::new(),
        };
        this.build_block_index()?;
        Ok(this)
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

        let mut this = Archive {
            header,
            bf,
            file: RefCell::new(file),
            blockchains: HashMap::with_capacity(16),
        };

        this.build_block_index()?;

        Ok(this)
    }

    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    fn build_block_index(&mut self) -> Result<()> {
        let mut offsets = vec![PK2_ROOT_BLOCK];
        while let Some(offset) = offsets.pop() {
            let block = self.read_block_chain_at(offset)?;
            for block in &block.blocks {
                for entry in &block.entries {
                    if let PackEntry::Folder {
                        name, pos_children, ..
                    } = entry
                    {
                        if name != "." && name != ".." {
                            offsets.push(*pos_children);
                        }
                    }
                }
            }
            self.blockchains.insert(offset, block);
        }
        Ok(())
    }

    fn read_block_chain_at(&mut self, offset: u64) -> Result<PackBlockChain> {
        let mut offset = offset;
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        let mut blocks = Vec::new();
        loop {
            {
                let mut file = self.file.borrow_mut();
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(&mut buf)?;
            }
            let _ = self.bf.decrypt_nopad(&mut buf);
            let block = PackBlock::from_reader(Cursor::new(&buf[..]), offset)?;
            let nc = block[19].next_chain();
            blocks.push(block);
            match nc {
                Some(nc) => offset = nc.get(),
                None => break Ok(PackBlockChain::new(blocks)),
            }
        }
    }
}

impl Archive {
    fn resolve_path_to_entry(&self, path: &Path) -> Result<(u64, usize, &PackEntry)> {
        if let Ok(path) = path.strip_prefix("/") {
            self.resolve_path_to_entry_at(PK2_ROOT_BLOCK, path)
        } else {
            Err(err_not_found("Absolute path expected".to_owned()))
        }
    }

    // code duplication yay
    fn resolve_path_to_block_chain(&self, path: &Path) -> Result<&PackBlockChain> {
        if let Ok(path) = path.strip_prefix("/") {
            self.resolve_path_to_block_chain_index_at(PK2_ROOT_BLOCK, path)
                .map(|idx| &self.blockchains[&idx])
        } else {
            Err(err_not_found("Absolute path expected".to_owned()))
        }
    }

    pub(crate) fn resolve_path_to_entry_at(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<(u64, usize, &PackEntry)> {
        let mut components = path.components();
        let name = component_to_str(components.next_back().unwrap()); // todo remove unwrap
        let chain =
            self.resolve_path_to_block_chain_index_at(current_chain, components.as_path())?;
        let (idx, entry) = self.blockchains[&chain]
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.name() == name)
            .ok_or_else(|| err_not_found(["Unable to find file ", name.unwrap()].join("")))?;
        Ok((chain, idx, entry))
    }

    pub(crate) fn resolve_path_to_block_chain_index_at(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<u64> {
        path.components().try_fold(current_chain, |idx, component| {
            self.find_block_chain_index_in(
                &self.blockchains[&idx],
                component_to_str(component).unwrap(),
            )
        })
    }

    fn find_block_chain_index_in(&self, chain: &PackBlockChain, folder: &str) -> Result<u64> {
        for entry in chain.iter() {
            return match entry {
                PackEntry::Folder {
                    name, pos_children, ..
                } if name == folder => Ok(*pos_children),
                PackEntry::File { name, .. } if name == folder => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Expected a directory, found a file",
                )),
                _ => continue,
            };
        }
        Err(err_not_found(
            ["Unable to find directory ", folder].join(""),
        ))
    }

    fn create_entry_at(&mut self, mut chain: u64, path: &Path) -> Result<(u64, usize)> {
        let mut components = path.components().peekable();
        // check how far of the path exists
        while let Some(component) = components.peek() {
            let name = component_to_str(*component).unwrap();
            match self.find_block_chain_index_in(&self.blockchains[&chain], name) {
                Ok(i) => {
                    chain = i;
                    let _ = components.next();
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                    if *component == Component::ParentDir {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "The path is a parent of the root directory",
                        ));
                    } else {
                        break;
                    }
                }
                // the current name already exists as a file
                Err(e) => {
                    return Err(e);
                }
            }
        }
        while let Some(component) = components.next() {
            let name = component_to_str(component).unwrap();
            match component {
                // use inserted indices
                Component::Normal(p) => {
                    let opt = self
                        .blockchains
                        .get_mut(&chain)
                        .unwrap()
                        .iter_mut()
                        .enumerate()
                        .find(|(_, entry)| entry.is_empty());
                    match opt {
                        Some((idx, entry)) => {
                            if let Some(c) = components.peek() {
                                let next_block_offset = self.file.borrow().metadata()?.len();
                                *entry = PackEntry::new_folder(
                                    component_to_str(*c).unwrap().to_owned(),
                                    next_block_offset,
                                    None,
                                );
                                let block = self.create_new_block_at(next_block_offset)?;
                                self.blockchains
                                    .insert(next_block_offset, PackBlockChain::new(vec![block]));
                            } else {
                                return Ok((chain, idx));
                            }
                        }
                        None => {
                            let next_block_offset = self.file.borrow().metadata()?.len();
                            let block = self.create_new_block_at(next_block_offset)?;
                            let block_chain = self.blockchains.get_mut(&chain).unwrap();
                            block_chain.blocks.last_mut().unwrap()[PK2_FILE_BLOCK_ENTRY_COUNT - 1]
                                .set_next_chain(next_block_offset);
                            block_chain.blocks.push(block);
                        }
                    }
                }
                _ => (),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Path already exists",
        ))
    }

    fn create_new_block_at(&mut self, offset: u64) -> Result<PackBlock> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        let mut block = PackBlock {
            offset: 0,
            entries: Default::default(),
        };
        block.to_writer(&mut Cursor::new(&mut buf[..]))?;
        let _ = self.bf.encrypt_nopad(&mut buf);
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buf)?;
        block.offset = offset;
        Ok(block)
    }
}

impl Archive {
    pub fn create_file<P: AsRef<Path>>(&mut self, path: P, size: u32) -> Result<FileMut> {
        if let Ok(path) = path.as_ref().strip_prefix("/") {
            self.create_entry_at(PK2_ROOT_BLOCK, path.as_ref())?;

            unimplemented!()
        } else {
            Err(err_not_found("Absolute path expected".to_owned()))
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let (_, _, entry) = self.resolve_path_to_entry(path.as_ref())?;
        Ok(File::new(self, entry))
    }

    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        let (chain, block, _) = self.resolve_path_to_entry(path.as_ref())?;
        Ok(FileMut::new(self, chain, block))
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
        let chain = self.resolve_path_to_block_chain(path.as_ref())?;
        Ok(Directory::new(self, chain))
    }

    /*
    pub fn open_dir_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<DirMut> {
        unimplemented!()
    }*/
}

fn component_to_str(component: Component) -> Option<&str> {
    match component {
        Component::Normal(p) => p.to_str(),
        Component::ParentDir => Some(".."),
        Component::CurDir => Some("."),
        _ => None,
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
