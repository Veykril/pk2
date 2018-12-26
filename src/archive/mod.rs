use block_modes::BlockMode;
use hashbrown::HashMap;

use std::cell::RefCell;
use std::fs::File as StdFile;
use std::io::{self, Cursor, Read, Result, Seek, SeekFrom};
use std::path::{Component, Path};

use crate::constants::*;
use crate::fs::{Directory, File};
use crate::Blowfish;

//pub mod pack_block;
pub mod block_chain;
pub mod entry;
pub mod header;

//use self::pack_block::PackBlock;
pub use self::block_chain::{PackBlock, PackBlockChain};
use self::entry::PackEntry;
pub use self::header::PackHeader;

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
        let header = PackHeader::new(&mut bf);

        header.to_writer(&mut file)?;

        Ok(Archive {
            header,
            bf,
            file: RefCell::new(file),
            blockchains: HashMap::new(),
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
            let mut checksum = PK2_CHECKSUM;
            let _ = bf.encrypt_nopad(&mut checksum);
            if checksum[..PK2_CHECKSUM_STORED] != header.verify[..PK2_CHECKSUM_STORED] {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid blowfish key",
                ))?;
            }
        }

        println!("{:?}", header);

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
    pub fn resolve_path_to_entry(&self, path: &Path) -> Result<&PackEntry> {
        let components = canonicalize(path)?;
        let (last, mut components) = (components.len() - 1, components.into_iter().enumerate());

        if let Some((_, Component::RootDir)) = components.next() {
            let mut current_chain = &self.blockchains[&PK2_ROOT_BLOCK];
            for (idx, component) in components {
                if let Component::Normal(p) = component {
                    let p = p.to_str().unwrap();
                    if idx == last {
                        return current_chain
                            .into_iter()
                            .find(|entry| entry.name() == Some(p))
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::NotFound,
                                    ["Unable to find file ", p].join(""),
                                )
                            });
                    } else {
                        current_chain = self.find_blockchain_in_blockchain(current_chain, p)?;
                    }
                }
            }
        }
        unimplemented!("relative paths havent been implemented yet");
    }

    pub fn resolve_path_to_block_chain(&self, path: &Path) -> Result<&PackBlockChain> {
        let mut components = canonicalize(path)?.into_iter();

        if let Some(Component::RootDir) = components.next() {
            let mut current_chain = &self.blockchains[&PK2_ROOT_BLOCK];
            for component in components {
                if let Component::Normal(p) = component {
                    current_chain =
                        self.find_blockchain_in_blockchain(current_chain, p.to_str().unwrap())?;
                }
            }
            Ok(current_chain)
        } else {
            unimplemented!("relative paths havent been implemented yet");
        }
    }

    fn find_blockchain_in_blockchain(
        &self,
        chain: &PackBlockChain,
        folder: &str,
    ) -> Result<&PackBlockChain> {
        for entry in chain {
            if let PackEntry::Folder {
                ref name,
                pos_children,
                ..
            } = entry
            {
                if name == folder {
                    return Ok(&self.blockchains[pos_children]);
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            ["Unable to find folder ", folder].join(""),
        ))
    }

    /*
    pub fn create_file<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        unimplemented!()
    }*/

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let entry = self.resolve_path_to_entry(path.as_ref())?;
        Ok(File::new(self, entry))
    }
    /*
    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        unimplemented!()
    }*/
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

fn canonicalize(path: &Path) -> Result<Vec<Component>> {
    let mut components = Vec::with_capacity(8);
    for c in path.components() {
        match c {
            Component::CurDir => continue,
            Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Cannot leave root",
                    ));
                }
            }
            c => components.push(c),
        }
    }
    Ok(components)
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
