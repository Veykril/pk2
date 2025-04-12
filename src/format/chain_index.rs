use alloc::vec::Vec;
use core::iter;
use core::num::NonZeroU64;

use hashbrown::HashMap;
use hashbrown::hash_map::Entry;
use rustc_hash::FxBuildHasher;

use crate::error::{ChainLookupError, ChainLookupResult};
use crate::format::block_chain::{PackBlock, PackBlockChain};
use crate::format::entry::{InvalidPackEntryType, PackEntry};
use crate::format::header::PackHeader;
use crate::format::{BlockOffset, ChainOffset};

/// Simple ChainIndex backed by a hashmap.
#[derive(Default)]
pub struct ChainIndex {
    chains: HashMap<ChainOffset, PackBlockChain, FxBuildHasher>,
}

pub struct ChainIndexParser<'bm> {
    chain_index: &'bm mut ChainIndex,
    offsets_to_process: Vec<(ChainOffset, BlockOffset)>,
}

impl<'bm> ChainIndexParser<'bm> {
    pub fn new(
        manager: &'bm mut ChainIndex,
        offsets_to_process: Vec<(ChainOffset, BlockOffset)>,
    ) -> Self {
        ChainIndexParser { chain_index: manager, offsets_to_process }
    }

    /// Abandon parsing, returning the unfinished work.
    pub fn abandon(self) -> Vec<(ChainOffset, BlockOffset)> {
        self.offsets_to_process
    }

    pub fn wants_read_at(&self) -> Option<BlockOffset> {
        self.offsets_to_process.last().map(|&(_, block)| block)
    }

    pub fn progress(
        &mut self,
        buffer: &[u8; PackBlock::PK2_FILE_BLOCK_SIZE],
    ) -> Result<usize, InvalidPackEntryType> {
        let Some((chain_index, block_offset)) = self.offsets_to_process.pop() else {
            return Ok(0);
        };

        let block = PackBlock::parse(buffer)?;

        if let Some(nb) = block.next_block() {
            self.offsets_to_process.push((chain_index, nb))
        }
        // put all folder offsets of this block into the stack to parse them next
        // Note, we might still put duplicate blocks on here which results in extra unnecessary
        // work.
        // This should only occur for a bad archives though.
        // The only expected duplicates are chain heads, like `..` and `.` which point back to the
        // parent or self. These we filter out appropriately below though.
        self.offsets_to_process.extend(
            block
                .entries()
                .filter_map(PackEntry::children)
                .filter(|chain| {
                    *chain != chain_index || !self.chain_index.chains.contains_key(chain)
                })
                .map(|chain @ ChainOffset(co)| (chain, BlockOffset(co))),
        );
        match self.chain_index.chains.entry(chain_index) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().push(block_offset, block)
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(PackBlockChain::from_blocks(vec![(block_offset, block)]));
            }
        }

        Ok(self.offsets_to_process.len())
    }
}

impl ChainIndex {
    pub const PK2_ROOT_CHAIN_OFFSET: ChainOffset =
        ChainOffset(NonZeroU64::new(PackHeader::PACK_HEADER_LEN as u64).unwrap());
    pub const PK2_ROOT_BLOCK_OFFSET: BlockOffset =
        BlockOffset(NonZeroU64::new(PackHeader::PACK_HEADER_LEN as u64).unwrap());

    #[cfg(feature = "std")]
    pub fn read_sync(
        r: &mut (impl std::io::Read + std::io::Seek),
        bf: Option<&crate::blowfish::Blowfish>,
    ) -> std::io::Result<Self> {
        let mut this = ChainIndex::default();
        let mut fsm = ChainIndexParser::new(
            &mut this,
            vec![(Self::PK2_ROOT_CHAIN_OFFSET, Self::PK2_ROOT_BLOCK_OFFSET)],
        );
        let mut buffer = [0; PackBlock::PK2_FILE_BLOCK_SIZE];
        while let Some(offset) = fsm.wants_read_at() {
            r.seek(std::io::SeekFrom::Start(offset.0.get()))?;
            r.read_exact(&mut buffer)?;
            if let Some(bf) = bf {
                bf.decrypt(&mut buffer);
            }
            fsm.progress(&buffer).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse block at offset {}: {}", offset.0, e),
                )
            })?;
        }
        this.chains.shrink_to_fit();
        Ok(this)
    }

    pub fn get(&self, chain: ChainOffset) -> Option<&PackBlockChain> {
        self.chains.get(&chain)
    }

    pub fn get_mut(&mut self, chain: ChainOffset) -> Option<&mut PackBlockChain> {
        self.chains.get_mut(&chain)
    }

    pub fn get_entry(&self, chain: ChainOffset, entry: usize) -> Option<&PackEntry> {
        self.chains.get(&chain)?.get(entry)
    }

    pub fn get_entry_mut(&mut self, chain: ChainOffset, entry: usize) -> Option<&mut PackEntry> {
        self.chains.get_mut(&chain)?.get_mut(entry)
    }

    pub fn insert(&mut self, chain: ChainOffset, block: PackBlockChain) {
        self.chains.insert(chain, block);
    }

    pub fn resolve_path_to_parent<'path>(
        &self,
        current_chain: Option<ChainOffset>,
        path: &'path str,
    ) -> ChainLookupResult<(ChainOffset, &'path str)> {
        let components = path.rsplit_once('/');

        if let Some((rest, name)) = components {
            if name.is_empty() {
                return Err(ChainLookupError::InvalidPath);
            }
            let parent_index = self.resolve_path_to_block_chain_index_at(current_chain, rest)?;
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
        current_chain: Option<ChainOffset>,
        path: &str,
    ) -> ChainLookupResult<(ChainOffset, usize, &PackEntry)> {
        self.resolve_path_to_parent(current_chain, path).and_then(|(parent_index, name)| {
            self.chains
                .get(&parent_index)
                .ok_or(ChainLookupError::InvalidChainOffset)?
                .entries()
                .enumerate()
                .find(|(_, entry)| entry.name_eq_ignore_ascii_case(name))
                .ok_or(ChainLookupError::NotFound)
                .map(|(idx, entry)| (parent_index, idx, entry))
        })
    }

    pub fn resolve_path_to_entry_and_parent_mut(
        &mut self,
        current_chain: Option<ChainOffset>,
        path: &str,
    ) -> ChainLookupResult<(ChainOffset, usize, &mut PackEntry)> {
        self.resolve_path_to_parent(current_chain, path).and_then(move |(parent_index, name)| {
            self.chains
                .get_mut(&parent_index)
                .ok_or(ChainLookupError::InvalidChainOffset)?
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
        current_chain: Option<ChainOffset>,
        path: &str,
    ) -> ChainLookupResult<ChainOffset> {
        path.split('/').try_fold(
            // FIXME: is this correct?
            current_chain.unwrap_or(Self::PK2_ROOT_CHAIN_OFFSET),
            |idx, component| {
                if component.is_empty() {
                    return Err(ChainLookupError::InvalidPath);
                }
                self.chains
                    .get(&idx)
                    .ok_or(ChainLookupError::InvalidChainOffset)?
                    .find_block_chain_index_of(component)
            },
        )
    }

    /// Traverses the path until it hits a non-existent component and returns
    /// the rest of the path as a peekable as well as the chain index of the
    /// last valid part.
    /// A return value of Ok(None) means the entire path has been searched
    pub fn validate_dir_path_until<'p>(
        &self,
        mut chain: ChainOffset,
        path: &'p str,
    ) -> ChainLookupResult<Option<(ChainOffset, iter::Peekable<core::str::Split<'p, char>>)>> {
        let mut components = path.split('/').peekable();
        while let Some(component) = components.peek() {
            if component.is_empty() {
                return Err(ChainLookupError::InvalidPath);
            }
            match self
                .chains
                .get(&chain)
                .ok_or(ChainLookupError::InvalidChainOffset)?
                .find_block_chain_index_of(component)
            {
                Ok(i) => chain = i,
                // found a non-existent part, we are done here
                Err(ChainLookupError::NotFound) => break,
                Err(e) => return Err(e),
            }
            let _ = components.next();
        }
        if components.clone().count() == 0 { Ok(None) } else { Ok(Some((chain, components))) }
    }
}
