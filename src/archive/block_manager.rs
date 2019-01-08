use hashbrown::HashMap;

use std::io::{self, Result};
use std::path::{Component, Path};

use crate::archive::{err_not_found, PackBlockChain, PackEntry, PhysFile};
use crate::constants::PK2_ROOT_BLOCK;

pub(in crate) struct BlockManager {
    pub(in crate) chains: HashMap<u64, PackBlockChain>,
}

impl BlockManager {
    pub(in crate) fn new(file: &mut PhysFile) -> Result<Self> {
        let mut chains = HashMap::new();
        let mut offsets = vec![PK2_ROOT_BLOCK];
        // eager population of the file index, cause lazy initialization would require either interior mutability or &mut self everywhere
        while let Some(offset) = offsets.pop() {
            let block = Self::read_chain_from_file_at(file, offset)?;
            for block in block.as_ref() {
                for entry in &block.entries {
                    if let PackEntry::Directory {
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

    /// Reads a [`PackBlockChain`] from the given reader `r` at the specified offset
    fn read_chain_from_file_at(file: &mut PhysFile, offset: u64) -> Result<PackBlockChain> {
        let mut offset = offset;
        let mut blocks = Vec::new();
        loop {
            let block = file.read_block_at(offset)?;
            let nc = block[19].next_chain();
            blocks.push(block);
            match nc {
                Some(nc) => offset = nc.get(),
                None => break Ok(PackBlockChain::new(blocks)),
            }
        }
    }

    /// Resolves a path from the specified chain to a parent chain and the entry
    /// Returns Ok(None) if the path is empty
    pub(in crate) fn resolve_path_to_entry_and_parent(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<Option<(&PackBlockChain, usize, &PackEntry)>> {
        let mut components = path.components();
        if let Some(c) = components.next_back() {
            let name = c.as_os_str().to_str();
            let parent = &self.chains[&self
                .resolve_path_to_block_chain_index_at(current_chain, components.as_path())?];
            parent
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.name() == name)
                .ok_or_else(|| err_not_found(["Unable to find file ", name.unwrap()].join("")))
                .map(|(idx, entry)| Some((parent, idx, entry)))
        } else {
            Ok(None)
        }
    }

    /// Resolves a path to a [`PackBlockChain`] index starting from the given chain
    pub(in crate) fn resolve_path_to_block_chain_index_at(
        &self,
        current_chain: u64,
        path: &Path,
    ) -> Result<u64> {
        path.components().try_fold(current_chain, |idx, component| {
            self.chains[&idx].find_block_chain_index_in(component.as_os_str().to_str().unwrap())
        })
    }

    /// checks the existence of the given path as a directory and returns the last existing chain
    /// and the non-existent rest of the path if left
    pub(in crate) fn validate_dir_path_until<'a>(
        &self,
        mut chain: u64,
        path: &'a Path,
    ) -> Result<(u64, &'a Path)> {
        let components = path.components();
        let mut n = 0;
        for component in components {
            let name = component.as_os_str().to_str().unwrap();
            match self.chains[&chain].find_block_chain_index_in(name) {
                Ok(i) => {
                    chain = i;
                    n += 1;
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                    if component == Component::ParentDir {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "The path is a parent of the root directory",
                        ));
                    } else {
                        break;
                    }
                }
                // the current name already exists as a file or something else happened
                // todo change the StringError("Expected a directory, found a file") error into something we can match on to change it here
                Err(e) => {
                    return Err(e);
                }
            }
        }
        let mut components = path.components();
        // get rid of the first n elements, is there a nicer way to do this?
        components.by_ref().take(n).next();
        Ok((chain, components.as_path()))
    }
}
