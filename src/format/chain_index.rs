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
#[derive(Default, Debug)]
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
                    *chain != chain_index && !self.chain_index.chains.contains_key(chain)
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
        current_chain: ChainOffset,
        path: &'path str,
    ) -> ChainLookupResult<(ChainOffset, &'path str)> {
        let components = path.rsplit_once(['/', '\\']);

        if let Some((rest, name)) = components {
            if name.is_empty() {
                return Err(ChainLookupError::InvalidPath);
            }
            let parent_index = self.resolve_path_to_block_chain_index_at(current_chain, rest)?;
            Ok((parent_index, name))
        } else {
            if path.is_empty() {
                return Err(ChainLookupError::InvalidPath);
            }
            Ok((current_chain, path))
        }
    }

    /// Resolves a path from the specified chain to a parent chain and the entry
    /// Returns Ok(None) if the path is empty, otherwise (blockchain,
    /// entry_index, entry)
    pub fn resolve_path_to_entry_and_parent(
        &self,
        current_chain: ChainOffset,
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
        current_chain: ChainOffset,
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
        current_chain: ChainOffset,
        path: &str,
    ) -> ChainLookupResult<ChainOffset> {
        path.split(['/', '\\']).try_fold(current_chain, |idx, component| {
            if component.is_empty() {
                return Err(ChainLookupError::InvalidPath);
            }
            self.chains
                .get(&idx)
                .ok_or(ChainLookupError::InvalidChainOffset)?
                .find_block_chain_index_of(component)
        })
    }

    /// Traverses the path until it hits a non-existent component and returns
    /// the rest of the path as a peekable as well as the chain index of the
    /// last valid part.
    /// A return value of Ok(None) means the entire path has been searched
    pub fn validate_dir_path_until<'p>(
        &self,
        mut chain: ChainOffset,
        path: &'p str,
    ) -> ChainLookupResult<
        Option<(ChainOffset, iter::Peekable<impl use<'p> + Iterator<Item = &'p str>>)>,
    > {
        let mut components = path.split(['/', '\\']).peekable();
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

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;

    use super::*;
    use crate::format::StreamOffset;
    use crate::format::entry::PackEntry;

    fn make_empty_block() -> PackBlock {
        PackBlock::default()
    }

    fn make_root_block_with_entries(entries: Vec<(usize, PackEntry)>) -> PackBlock {
        let mut block = PackBlock::default();
        // Standard root block has "." and ".." entries
        block[0] = PackEntry::new_directory(".", ChainOffset(NonZeroU64::new(256).unwrap()), None);
        block[1] = PackEntry::new_directory("..", ChainOffset(NonZeroU64::new(256).unwrap()), None);
        for (idx, entry) in entries {
            block[idx] = entry;
        }
        block
    }

    #[test]
    fn chain_index_insert_and_get() {
        let mut index = ChainIndex::default();
        let chain_offset = ChainOffset(NonZeroU64::new(256).unwrap());
        let block = make_empty_block();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        index.insert(chain_offset, chain);

        assert!(index.get(chain_offset).is_some());
    }

    #[test]
    fn chain_index_get_mut() {
        let mut index = ChainIndex::default();
        let chain_offset = ChainOffset(NonZeroU64::new(256).unwrap());
        let block = make_empty_block();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        index.insert(chain_offset, chain);

        let chain_mut = index.get_mut(chain_offset);
        assert!(chain_mut.is_some());
    }

    #[test]
    fn chain_index_get_entry() {
        let mut index = ChainIndex::default();
        let chain_offset = ChainOffset(NonZeroU64::new(256).unwrap());
        let mut block = make_empty_block();
        block[5] = PackEntry::new_file(
            "test.txt",
            StreamOffset(NonZeroU64::new(1000).unwrap()),
            100,
            None,
        );
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        index.insert(chain_offset, chain);

        let entry = index.get_entry(chain_offset, 5);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name(), Some("test.txt"));
    }

    #[test]
    fn chain_index_get_entry_mut() {
        let mut index = ChainIndex::default();
        let chain_offset = ChainOffset(NonZeroU64::new(256).unwrap());
        let block = make_empty_block();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        index.insert(chain_offset, chain);

        {
            let entry = index.get_entry_mut(chain_offset, 3).unwrap();
            *entry = PackEntry::new_file(
                "modified.txt",
                StreamOffset(NonZeroU64::new(500).unwrap()),
                50,
                None,
            );
        }

        assert_eq!(index.get_entry(chain_offset, 3).unwrap().name(), Some("modified.txt"));
    }

    #[test]
    fn chain_index_get_entry_invalid_chain() {
        let index = ChainIndex::default();
        let invalid_offset = ChainOffset(NonZeroU64::new(9999).unwrap());
        assert!(index.get_entry(invalid_offset, 0).is_none());
    }

    #[test]
    fn chain_index_get_entry_invalid_index() {
        let mut index = ChainIndex::default();
        let chain_offset = ChainOffset(NonZeroU64::new(256).unwrap());
        let block = make_empty_block();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        index.insert(chain_offset, chain);

        assert!(index.get_entry(chain_offset, 100).is_none());
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_single_component() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let subdir_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("subdir", subdir_offset, None),
        )]);
        let subdir_block = make_empty_block();

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            subdir_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                subdir_block,
            )]),
        );

        let result =
            index.resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "subdir");
        assert_eq!(result, Ok(subdir_offset));
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_nested_path() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());
        let dir2_offset = ChainOffset(NonZeroU64::new(8000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);
        dir1_block[2] = PackEntry::new_directory("dir2", dir2_offset, None);

        let dir2_block = make_empty_block();

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );
        index.insert(
            dir2_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(8000).unwrap()),
                dir2_block,
            )]),
        );

        let result = index
            .resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "dir1/dir2");
        assert_eq!(result, Ok(dir2_offset));
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_case_insensitive() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let subdir_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("SubDir", subdir_offset, None),
        )]);
        let subdir_block = make_empty_block();

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            subdir_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                subdir_block,
            )]),
        );

        // Should find with different case
        assert_eq!(
            index.resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "subdir"),
            Ok(subdir_offset)
        );
        assert_eq!(
            index.resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "SUBDIR"),
            Ok(subdir_offset)
        );
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_not_found() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let root_block = make_root_block_with_entries(vec![]);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );

        let result = index
            .resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "nonexistent");
        assert_eq!(result, Err(ChainLookupError::NotFound));
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_empty_component() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        // Create root with dir1 so we can reach the empty component
        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );

        // Path with empty component (double slash) - after dir1 exists, the empty component is detected
        let result = index
            .resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "dir1//dir2");
        assert_eq!(result, Err(ChainLookupError::InvalidPath));
    }

    #[test]
    fn resolve_path_to_block_chain_index_at_backslash_separator() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());
        let dir2_offset = ChainOffset(NonZeroU64::new(8000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);
        dir1_block[2] = PackEntry::new_directory("dir2", dir2_offset, None);

        let dir2_block = make_empty_block();

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );
        index.insert(
            dir2_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(8000).unwrap()),
                dir2_block,
            )]),
        );

        // Should work with backslash separator (Windows-style)
        let result = index
            .resolve_path_to_block_chain_index_at(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "dir1\\dir2");
        assert_eq!(result, Ok(dir2_offset));
    }

    // resolve_path_to_parent tests

    #[test]
    fn resolve_path_to_parent_simple() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let root_block = make_root_block_with_entries(vec![]);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );

        let result =
            index.resolve_path_to_parent(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "dir/file.txt");
        // Should fail because "dir" doesn't exist
        assert!(result.is_err());
    }

    #[test]
    fn resolve_path_to_parent_no_slash() {
        let index = ChainIndex::default();
        // Single-component paths should work - parent is root, name is the component
        let result = index.resolve_path_to_parent(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "file.txt");
        assert_eq!(result, Ok((ChainIndex::PK2_ROOT_CHAIN_OFFSET, "file.txt")));
    }

    #[test]
    fn resolve_path_to_parent_empty_path() {
        let index = ChainIndex::default();
        // Empty path should fail
        let result = index.resolve_path_to_parent(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "");
        assert_eq!(result, Err(ChainLookupError::InvalidPath));
    }

    #[test]
    fn resolve_path_to_parent_trailing_slash() {
        let index = ChainIndex::default();
        let result = index.resolve_path_to_parent(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "dir/");
        assert_eq!(result, Err(ChainLookupError::InvalidPath));
    }

    #[test]
    fn resolve_path_to_entry_and_parent_file() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_file(
                "test.txt",
                StreamOffset(NonZeroU64::new(10000).unwrap()),
                500,
                None,
            ),
        )]);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );

        let result = index
            .resolve_path_to_entry_and_parent(ChainIndex::PK2_ROOT_CHAIN_OFFSET, "root/test.txt");
        // This should fail because "root" doesn't exist as a directory
        // The path resolution expects the first component to be found
        assert!(result.is_err());
    }

    #[test]
    fn resolve_path_to_entry_and_parent_in_subdir() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let subdir_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("subdir", subdir_offset, None),
        )]);

        let mut subdir_block = make_empty_block();
        subdir_block[0] = PackEntry::new_directory(".", subdir_offset, None);
        subdir_block[1] = PackEntry::new_directory("..", root_offset, None);
        subdir_block[2] = PackEntry::new_file(
            "myfile.txt",
            StreamOffset(NonZeroU64::new(10000).unwrap()),
            200,
            None,
        );

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            subdir_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                subdir_block,
            )]),
        );

        let result = index.resolve_path_to_entry_and_parent(
            ChainIndex::PK2_ROOT_CHAIN_OFFSET,
            "subdir/myfile.txt",
        );
        assert!(result.is_ok());
        let (parent_chain, entry_idx, entry) = result.unwrap();
        assert_eq!(parent_chain, subdir_offset);
        assert_eq!(entry_idx, 2);
        assert_eq!(entry.name(), Some("myfile.txt"));
    }

    #[test]
    fn resolve_path_to_entry_and_parent_not_found() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let subdir_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("subdir", subdir_offset, None),
        )]);

        let mut subdir_block = make_empty_block();
        subdir_block[0] = PackEntry::new_directory(".", subdir_offset, None);
        subdir_block[1] = PackEntry::new_directory("..", root_offset, None);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            subdir_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                subdir_block,
            )]),
        );

        let result = index.resolve_path_to_entry_and_parent(
            ChainIndex::PK2_ROOT_CHAIN_OFFSET,
            "subdir/nonexistent.txt",
        );
        assert_eq!(result, Err(ChainLookupError::NotFound));
    }

    #[test]
    fn validate_dir_path_until_all_exists() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());
        let dir2_offset = ChainOffset(NonZeroU64::new(8000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);
        dir1_block[2] = PackEntry::new_directory("dir2", dir2_offset, None);

        let dir2_block = make_empty_block();

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );
        index.insert(
            dir2_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(8000).unwrap()),
                dir2_block,
            )]),
        );

        // All components exist, should return None
        let result = index.validate_dir_path_until(root_offset, "dir1/dir2");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn validate_dir_path_until_partial() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );

        // dir1 exists but dir2/dir3 don't
        let result = index.validate_dir_path_until(root_offset, "dir1/dir2/dir3");
        assert!(result.is_ok());
        let (chain, mut remaining) = result.unwrap().unwrap();
        assert_eq!(chain, dir1_offset);
        assert_eq!(remaining.next(), Some("dir2"));
        assert_eq!(remaining.next(), Some("dir3"));
        assert_eq!(remaining.next(), None);
    }

    #[test]
    fn validate_dir_path_until_none_exist() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let root_block = make_root_block_with_entries(vec![]);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );

        let result = index.validate_dir_path_until(root_offset, "new1/new2/new3");
        assert!(result.is_ok());
        let (chain, mut remaining) = result.unwrap().unwrap();
        assert_eq!(chain, root_offset);
        assert_eq!(remaining.next(), Some("new1"));
        assert_eq!(remaining.next(), Some("new2"));
        assert_eq!(remaining.next(), Some("new3"));
    }

    #[test]
    fn validate_dir_path_until_invalid_path() {
        let mut index = ChainIndex::default();
        let root_offset = ChainIndex::PK2_ROOT_CHAIN_OFFSET;
        let dir1_offset = ChainOffset(NonZeroU64::new(5000).unwrap());

        // Create root with dir1 so we can reach the empty component
        let root_block = make_root_block_with_entries(vec![(
            2,
            PackEntry::new_directory("dir1", dir1_offset, None),
        )]);

        let mut dir1_block = make_empty_block();
        dir1_block[0] = PackEntry::new_directory(".", dir1_offset, None);
        dir1_block[1] = PackEntry::new_directory("..", root_offset, None);

        index.insert(
            root_offset,
            PackBlockChain::from_blocks(vec![(ChainIndex::PK2_ROOT_BLOCK_OFFSET, root_block)]),
        );
        index.insert(
            dir1_offset,
            PackBlockChain::from_blocks(vec![(
                BlockOffset(NonZeroU64::new(5000).unwrap()),
                dir1_block,
            )]),
        );

        // Empty component in path - after dir1 exists, the empty component is detected
        let result = index.validate_dir_path_until(root_offset, "dir1//dir2");
        assert!(matches!(result, Err(ChainLookupError::InvalidPath)));
    }

    #[test]
    fn chain_index_parser_wants_read_at() {
        let mut index = ChainIndex::default();
        let offsets = vec![(ChainIndex::PK2_ROOT_CHAIN_OFFSET, ChainIndex::PK2_ROOT_BLOCK_OFFSET)];
        let parser = ChainIndexParser::new(&mut index, offsets);

        assert_eq!(parser.wants_read_at(), Some(ChainIndex::PK2_ROOT_BLOCK_OFFSET));
    }

    #[test]
    fn chain_index_parser_wants_read_at_empty() {
        let mut index = ChainIndex::default();
        let parser = ChainIndexParser::new(&mut index, vec![]);

        assert!(parser.wants_read_at().is_none());
    }

    #[test]
    fn chain_index_parser_abandon() {
        let mut index = ChainIndex::default();
        let offsets = vec![
            (ChainIndex::PK2_ROOT_CHAIN_OFFSET, ChainIndex::PK2_ROOT_BLOCK_OFFSET),
            (
                ChainOffset(NonZeroU64::new(5000).unwrap()),
                BlockOffset(NonZeroU64::new(5000).unwrap()),
            ),
        ];
        let parser = ChainIndexParser::new(&mut index, offsets.clone());

        let abandoned = parser.abandon();
        assert_eq!(abandoned.len(), 2);
    }

    #[test]
    fn chain_index_parser_progress_empty_block() {
        let mut index = ChainIndex::default();
        let offsets = vec![(ChainIndex::PK2_ROOT_CHAIN_OFFSET, ChainIndex::PK2_ROOT_BLOCK_OFFSET)];
        let mut parser = ChainIndexParser::new(&mut index, offsets);

        let buffer = [0u8; PackBlock::PK2_FILE_BLOCK_SIZE];
        let remaining = parser.progress(&buffer).unwrap();

        // No more work to do after parsing empty block
        assert_eq!(remaining, 0);

        // Chain should be inserted
        assert!(index.get(ChainIndex::PK2_ROOT_CHAIN_OFFSET).is_some());
    }

    #[test]
    fn chain_index_parser_progress_discovers_subdirs() {
        let mut index = ChainIndex::default();
        let offsets = vec![(ChainIndex::PK2_ROOT_CHAIN_OFFSET, ChainIndex::PK2_ROOT_BLOCK_OFFSET)];
        let mut parser = ChainIndexParser::new(&mut index, offsets);

        // Create a block with a subdirectory
        let mut block = PackBlock::default();
        let subdir_offset = ChainOffset(NonZeroU64::new(5000).unwrap());
        block[0] = PackEntry::new_directory(".", ChainIndex::PK2_ROOT_CHAIN_OFFSET, None);
        block[1] = PackEntry::new_directory("..", ChainIndex::PK2_ROOT_CHAIN_OFFSET, None);
        block[2] = PackEntry::new_directory("subdir", subdir_offset, None);

        let mut buffer = [0u8; PackBlock::PK2_FILE_BLOCK_SIZE];
        block.write_to(&mut buffer);

        let remaining = parser.progress(&buffer).unwrap();

        // Should have discovered the subdirectory and added it to the work queue
        assert_eq!(remaining, 1);
        assert_eq!(parser.wants_read_at(), Some(BlockOffset(NonZeroU64::new(5000).unwrap())));
    }
}
