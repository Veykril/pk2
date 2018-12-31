use block_modes::BlockMode;
use hashbrown::HashMap;

use std::io::{Cursor, Read, Result, Seek, SeekFrom};
use std::path::Path;

use crate::archive::{err_not_found, PackBlock, PackBlockChain, PackEntry};
use crate::constants::{PK2_FILE_BLOCK_SIZE, PK2_ROOT_BLOCK};
use crate::{Blowfish, PackIndex};
use std::io;
use std::iter::Peekable;
use std::path::Component;
use std::path::Components;

pub struct BlockManager {
    pub chains: HashMap<u64, PackBlockChain>,
}

impl BlockManager {
    pub fn new<R: Read + Seek>(bf: &mut Blowfish, mut r: R) -> Result<Self> {
        let mut chains = HashMap::new();

        let mut offsets = vec![PK2_ROOT_BLOCK];
        while let Some(offset) = offsets.pop() {
            let block = Self::read_block_chain_at_from_file(bf, &mut r, offset)?;
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
            chains.insert(offset, block);
        }
        Ok(BlockManager { chains })
    }

    fn read_block_chain_at_from_file<R: Read + Seek>(
        bf: &mut Blowfish,
        mut r: R,
        offset: u64,
    ) -> Result<PackBlockChain> {
        let mut offset = offset;
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        let mut blocks = Vec::new();
        loop {
            r.seek(SeekFrom::Start(offset))?;
            r.read_exact(&mut buf)?;
            let _ = bf.decrypt_nopad(&mut buf);
            let block = PackBlock::from_reader(Cursor::new(&buf[..]), offset)?;
            let nc = block[19].next_chain();
            blocks.push(block);
            match nc {
                Some(nc) => offset = nc.get(),
                None => break Ok(PackBlockChain::new(blocks)),
            }
        }
    }

    /// Resolves a path from the specified chain to a parent chain, entry index and the entry
    pub(crate) fn resolve_path_to_entry_and_parent(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<Option<(PackIndex, &PackEntry)>> {
        let mut components = path.components();
        if let Some(c) = components.next_back() {
            let name = c.as_os_str().to_str();
            let parent =
                self.resolve_path_to_block_chain_index_at(current_chain, components.as_path())?;
            let (parent_idx, chain) = self.chains[&parent]
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.name() == name)
                .ok_or_else(|| err_not_found(["Unable to find file ", name.unwrap()].join("")))?;
            Ok(Some(((parent, parent_idx), chain)))
        } else {
            Ok(None)
        }
    }

    /// Resolves a path to a [`PackBlockChain`] index starting from the given chain
    pub(crate) fn resolve_path_to_block_chain_index_at(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<u64> {
        path.components().try_fold(current_chain, |idx, component| {
            self.chains[&idx].find_block_chain_index_in(component.as_os_str().to_str().unwrap())
        })
    }

    /// checks the existence of the given path as a directory and returns the last existing chain
    /// and and non-existent rest of the path
    pub fn validate_dir_path_until<'a>(
        &self,
        mut chain: u64,
        path: &'a Path,
    ) -> Result<(u64, Peekable<Components<'a>>)> {
        let mut components = path.components().peekable();
        while let Some(component) = components.peek() {
            let name = component.as_os_str().to_str().unwrap();
            match self.chains[&chain].find_block_chain_index_in(name) {
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
        Ok((chain, components))
    }
}
