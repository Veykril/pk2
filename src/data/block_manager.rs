use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path};

use crate::blowfish::Blowfish;
use crate::constants::{PK2_FILE_BLOCK_ENTRY_COUNT, PK2_ROOT_BLOCK, PK2_ROOT_BLOCK_VIRTUAL};
use crate::data::block_chain::{PackBlock, PackBlockChain};
use crate::data::entry::{NonEmptyEntry, PackEntry};
use crate::data::{BlockOffset, ChainIndex};
use crate::error::{ChainLookupError, ChainLookupResult, OpenResult};

/// Simple BlockManager backed by a hashmap.
pub struct BlockManager {
    chains: HashMap<ChainIndex, PackBlockChain, NoHashHasherBuilder>,
}

impl BlockManager {
    /// Parses the complete index of a pk2 file
    pub fn new<F: io::Read + io::Seek>(bf: Option<&Blowfish>, mut stream: F) -> OpenResult<Self> {
        let mut chains = HashMap::with_capacity_and_hasher(32, NoHashHasherBuilder);
        // used to prevent an infinite loop that can be caused by specific files
        let mut visited_block_set = HashSet::with_capacity_and_hasher(32, NoHashHasherBuilder);
        let mut offsets = vec![PK2_ROOT_BLOCK];
        while let Some(offset) = offsets.pop() {
            if chains.contains_key(&offset) {
                // skip offsets that are being pointed to multiple times
                continue;
            }
            let block_chain =
                Self::read_chain_from_stream_at(&mut visited_block_set, bf, &mut stream, offset)?;
            visited_block_set.clear();

            // put all folder offsets of this chain into the stack to parse them next
            offsets.extend(
                block_chain
                    .entries()
                    .filter_map(PackEntry::as_non_empty)
                    .filter(|d| d.is_normal_link())
                    .filter_map(NonEmptyEntry::directory_children_position),
            );
            chains.insert(offset, block_chain);
        }
        let mut this = BlockManager { chains };
        this.insert_virtual_root();
        Ok(this)
    }

    fn insert_virtual_root(&mut self) {
        // dummy entry to give root a proper name
        let mut virtual_root = PackBlockChain::from_blocks(vec![(
            PK2_ROOT_BLOCK_VIRTUAL.into(),
            PackBlock::default(),
        )]);
        virtual_root[0] = PackEntry::new_directory("/", PK2_ROOT_BLOCK, None);
        self.chains.insert(virtual_root.chain_index(), virtual_root);
    }

    /// Reads a [`PackBlockChain`] from the given file at the specified offset.
    fn read_chain_from_stream_at<F: io::Read + io::Seek>(
        visited_block_set: &mut HashSet<BlockOffset, NoHashHasherBuilder>,
        bf: Option<&Blowfish>,
        stream: &mut F,
        offset: ChainIndex,
    ) -> OpenResult<PackBlockChain> {
        let mut blocks = Vec::new();
        let mut offset = offset.into();

        while visited_block_set.insert(offset) {
            let block = crate::io::read_block_at(bf, &mut *stream, offset)?;
            let nc = block.entries().last().and_then(PackEntry::next_block);
            blocks.push((offset, block));
            match nc {
                Some(nc) => offset = BlockOffset(nc.get()),
                None => break,
            }
        }
        Ok(PackBlockChain::from_blocks(blocks))
    }

    pub fn get(&self, chain: ChainIndex) -> Option<&PackBlockChain> {
        self.chains.get(&chain)
    }

    pub fn get_mut(&mut self, chain: ChainIndex) -> Option<&mut PackBlockChain> {
        assert_ne!(chain, PK2_ROOT_BLOCK_VIRTUAL);
        self.chains.get_mut(&chain)
    }

    pub fn insert(&mut self, chain: ChainIndex, block: PackBlockChain) {
        self.chains.insert(chain, block);
    }

    pub fn resolve_path_to_parent<'path>(
        &self,
        current_chain: ChainIndex,
        path: &'path Path,
    ) -> ChainLookupResult<(ChainIndex, &'path str)> {
        let mut components = path.components();

        if let Some(c) = components.next_back() {
            let parent_index =
                self.resolve_path_to_block_chain_index_at(current_chain, components.as_path())?;
            let name = c.as_os_str().to_str().ok_or(ChainLookupError::InvalidPath)?;
            Ok((parent_index, name))
        } else {
            Err(ChainLookupError::InvalidPath)
        }
    }

    /// Resolves a path from the specified chain to a parent chain and the entry
    /// Returns Ok(None) if the path is empty, otherwise (blockchain,
    /// entry_index, entry)
    pub fn resolve_path_to_entry_and_parent(
        &self,
        current_chain: ChainIndex,
        path: &Path,
    ) -> ChainLookupResult<(ChainIndex, usize, &PackEntry)> {
        self.resolve_path_to_parent(current_chain, path).and_then(|(parent_index, name)| {
            self.chains
                .get(&parent_index)
                .ok_or(ChainLookupError::InvalidChainIndex)?
                .entries()
                .enumerate()
                .find(|(_, entry)| entry.name_eq_ignore_ascii_case(name))
                .ok_or(ChainLookupError::NotFound)
                .map(|(idx, entry)| (parent_index, idx, entry))
        })
    }

    pub fn resolve_path_to_entry_and_parent_mut(
        &mut self,
        current_chain: ChainIndex,
        path: &Path,
    ) -> ChainLookupResult<(ChainIndex, usize, &mut PackEntry)> {
        self.resolve_path_to_parent(current_chain, path).and_then(move |(parent_index, name)| {
            self.chains
                .get_mut(&parent_index)
                .ok_or(ChainLookupError::InvalidChainIndex)?
                .entries_mut()
                .enumerate()
                .find(|(_, entry)| entry.name_eq_ignore_ascii_case(name))
                .ok_or(ChainLookupError::NotFound)
                .map(|(idx, entry)| (parent_index, idx, entry))
        })
    }

    /// Resolves a path to a [`PackBlockChain`] index starting from the given
    /// blockchain returning the index of the last blockchain.
    pub fn resolve_path_to_block_chain_index_at(
        &self,
        current_chain: ChainIndex,
        path: &Path,
    ) -> ChainLookupResult<ChainIndex> {
        path.components().try_fold(current_chain, |idx, component| {
            let comp = component.as_os_str().to_str().ok_or(ChainLookupError::InvalidPath)?;
            self.chains
                .get(&idx)
                .ok_or(ChainLookupError::InvalidChainIndex)?
                .find_block_chain_index_of(comp)
        })
    }

    /// Traverses the path until it hits a non-existent component and returns
    /// the rest of the path as a peekable as well as the chain index of the
    /// last valid part.
    /// A return value of Ok(None) means the entire path has been searched
    pub fn validate_dir_path_until<'p>(
        &self,
        mut chain: ChainIndex,
        path: &'p Path,
    ) -> ChainLookupResult<Option<(ChainIndex, std::iter::Peekable<std::path::Components<'p>>)>>
    {
        let mut components = path.components().peekable();
        while let Some(component) = components.peek() {
            let name = component.as_os_str().to_str().ok_or(ChainLookupError::InvalidPath)?;
            match self
                .chains
                .get(&chain)
                .ok_or(ChainLookupError::InvalidChainIndex)?
                .find_block_chain_index_of(name)
            {
                Ok(i) => chain = i,
                // lies outside of the archive
                Err(ChainLookupError::NotFound) if component == &Component::ParentDir => {
                    return Err(ChainLookupError::InvalidPath);
                }
                // found a non-existent part, we are done here
                Err(ChainLookupError::NotFound) => break,
                Err(ChainLookupError::ExpectedDirectory) => {
                    return if components.count() == 1 {
                        // found a file name at the end of the path
                        // this means the path has been fully searched
                        Ok(None)
                    } else {
                        Err(ChainLookupError::ExpectedDirectory)
                    };
                }
                Err(_) => unreachable!(),
            }
            let _ = components.next();
        }
        if components.clone().count() == 0 { Ok(None) } else { Ok(Some((chain, components))) }
    }

    pub fn sort(&mut self) {
        let scratch = &mut Vec::with_capacity(4 * PK2_FILE_BLOCK_ENTRY_COUNT);
        for chain in self.chains.values_mut() {
            chain.sort(scratch);
            scratch.clear();
        }
    }
}

#[derive(Default)]
struct NoHashHasherBuilder;
impl std::hash::BuildHasher for NoHashHasherBuilder {
    type Hasher = NoHashHasher;
    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        NoHashHasher(0)
    }
}

struct NoHashHasher(u64);
impl std::hash::Hasher for NoHashHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        panic!("ChainIndex has been hashed wrong. This is a bug!");
    }

    #[inline(always)]
    fn write_u64(&mut self, chain: u64) {
        self.0 = chain;
    }
}
