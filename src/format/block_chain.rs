use alloc::vec::Vec;
use core::iter::zip;
use core::num::NonZeroU64;
use core::{ops, slice};

use crate::error::{ChainLookupError, ChainLookupResult};
use crate::format::entry::{InvalidPackEntryType, NonEmptyEntry, PackEntry};
use crate::format::{BlockOffset, ChainOffset, StreamOffset};

/// A collection of [`PackBlock`]s where each block's next_block field points to
/// the following block in the file. A PackBlockChain is never empty.
#[derive(Debug)]
pub struct PackBlockChain {
    blocks: Vec<(BlockOffset, PackBlock)>,
}

impl PackBlockChain {
    /// # Panics
    ///
    /// Panics if the blocks vector is empty.
    pub fn from_blocks(blocks: Vec<(BlockOffset, PackBlock)>) -> Self {
        assert!(!blocks.is_empty());
        PackBlockChain { blocks }
    }

    pub fn push_and_link(&mut self, offset: BlockOffset, block: PackBlock) {
        self.last_entry_mut().set_next_block(offset);
        self.blocks.push((offset, block));
    }

    pub fn push(&mut self, offset: BlockOffset, block: PackBlock) {
        assert_eq!(self.last_entry_mut().next_block(), Some(offset));
        self.blocks.push((offset, block));
    }

    /// This blockchains chain index/file offset.
    /// Note: This is the same as its first block
    pub fn chain_index(&self) -> ChainOffset {
        ChainOffset((self.blocks[0].0).0)
    }

    /// Returns the file offset of the entry at the given idx in this block
    /// chain.
    pub fn stream_offset_for_entry(&self, idx: usize) -> Option<StreamOffset> {
        self.blocks.get(idx / PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT).and_then(
            |(BlockOffset(offset), _)| {
                NonZeroU64::new(
                    offset.get()
                        + (PackEntry::PK2_FILE_ENTRY_SIZE
                            * (idx % PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT))
                            as u64,
                )
                .map(StreamOffset)
            },
        )
    }

    /// Returns the number of PackEntries in this chain.
    pub fn num_entries(&self) -> usize {
        self.blocks.len() * PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT
    }

    /// Returns the last entry of this PackBlockChain.
    pub fn last_entry_mut(&mut self) -> &mut PackEntry {
        let last = self.num_entries() - 1;
        &mut self[last]
    }

    /// An iterator over the entries of this chain.
    pub fn entries(&self) -> impl Iterator<Item = &PackEntry> {
        self.blocks.iter().flat_map(|block| &block.1.entries)
    }

    /// An iterator over the entries of this chain.
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut PackEntry> {
        self.blocks.iter_mut().flat_map(|block| &mut block.1.entries)
    }

    /// Get the PackEntry at the specified offset.
    pub fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.blocks
            .get(entry / PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|(_, block)| block.get(entry % PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    /// Get the PackEntry at the specified offset.
    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.blocks
            .get_mut(entry / PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|(_, block)| block.get_mut(entry % PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    pub fn contains_entry_index(&self, entry: usize) -> bool {
        entry < self.num_entries()
    }

    /// Looks up the `directory` name in this [`PackBlockChain`], returning the
    /// offset of the ['PackBlockChain'] corresponding to the directory if
    /// successful.
    pub fn find_block_chain_index_of(&self, directory: &str) -> ChainLookupResult<ChainOffset> {
        self.entries()
            .find(|entry| entry.name_eq_ignore_ascii_case(directory))
            .ok_or(ChainLookupError::NotFound)?
            .as_non_empty()
            .and_then(NonEmptyEntry::directory_children_offset)
            .ok_or(ChainLookupError::NotFound)
    }
}

impl ops::Index<usize> for PackBlockChain {
    type Output = PackEntry;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.blocks[idx / PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT].1
            [idx % PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

impl ops::IndexMut<usize> for PackBlockChain {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.blocks[idx / PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT].1
            [idx % PackBlock::PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

/// A collection of 20 [`PackEntry`]s.
#[derive(Default, Debug)]
pub struct PackBlock {
    entries: [PackEntry; Self::PK2_FILE_BLOCK_ENTRY_COUNT],
}

impl PackBlock {
    pub const PK2_FILE_BLOCK_ENTRY_COUNT: usize = 20;
    pub const PK2_FILE_BLOCK_SIZE: usize =
        PackEntry::PK2_FILE_ENTRY_SIZE * Self::PK2_FILE_BLOCK_ENTRY_COUNT;

    pub fn entries(&self) -> slice::Iter<'_, PackEntry> {
        self.entries.iter()
    }

    pub fn entries_mut(&mut self) -> slice::IterMut<'_, PackEntry> {
        self.entries.iter_mut()
    }

    pub fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.entries.get(entry)
    }

    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.entries.get_mut(entry)
    }

    pub fn next_block(&self) -> Option<BlockOffset> {
        self.entries[Self::PK2_FILE_BLOCK_ENTRY_COUNT - 1].next_block()
    }

    pub fn parse(buffer: &[u8; Self::PK2_FILE_BLOCK_SIZE]) -> Result<Self, InvalidPackEntryType> {
        let mut entries: [PackEntry; Self::PK2_FILE_BLOCK_ENTRY_COUNT] = Default::default();
        for (entry, buffer) in
            zip(&mut entries, buffer.chunks_exact(PackEntry::PK2_FILE_ENTRY_SIZE))
        {
            *entry = PackEntry::parse(buffer.try_into().unwrap())?;
        }
        Ok(PackBlock { entries })
    }

    pub fn write_to(&self, buffer: &mut [u8; Self::PK2_FILE_BLOCK_SIZE]) {
        for (entry, buffer) in
            zip(&self.entries, buffer.chunks_exact_mut(PackEntry::PK2_FILE_ENTRY_SIZE))
        {
            entry.write_to(buffer.try_into().unwrap());
        }
    }
}

impl ops::Index<usize> for PackBlock {
    type Output = PackEntry;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.entries[idx]
    }
}

impl ops::IndexMut<usize> for PackBlock {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.entries[idx]
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;

    use super::*;

    #[test]
    fn pack_block_parse_empty_block() {
        let buffer = [0u8; PackBlock::PK2_FILE_BLOCK_SIZE];
        let block = PackBlock::parse(&buffer).unwrap();
        for entry in block.entries() {
            assert!(entry.is_empty());
        }
    }

    #[test]
    fn pack_block_write_read_roundtrip() {
        let mut block = PackBlock::default();

        // Set up some entries
        block[0] =
            PackEntry::new_directory("testdir", ChainOffset(NonZeroU64::new(1000).unwrap()), None);
        block[1] = PackEntry::new_file(
            "testfile.txt",
            StreamOffset(NonZeroU64::new(2000).unwrap()),
            500,
            None,
        );
        block[19] = PackEntry::new_empty(NonZeroU64::new(5000).map(BlockOffset));

        let mut buffer = [0u8; PackBlock::PK2_FILE_BLOCK_SIZE];
        block.write_to(&mut buffer);

        let parsed = PackBlock::parse(&buffer).unwrap();

        assert!(parsed[0].is_directory());
        assert_eq!(parsed[0].name(), Some("testdir"));
        assert!(parsed[1].is_file());
        assert_eq!(parsed[1].name(), Some("testfile.txt"));
        assert!(parsed[2].is_empty());
        assert_eq!(parsed[19].next_block(), NonZeroU64::new(5000).map(BlockOffset));
    }

    #[test]
    #[should_panic]
    fn pack_block_chain_from_empty_blocks_panics() {
        let _ = PackBlockChain::from_blocks(vec![]);
    }

    #[test]
    fn pack_block_chain_chain_index() {
        let block = PackBlock::default();
        let offset = BlockOffset(NonZeroU64::new(1234).unwrap());
        let chain = PackBlockChain::from_blocks(vec![(offset, block)]);

        // chain_index should equal the first block's offset
        assert_eq!(chain.chain_index(), ChainOffset(NonZeroU64::new(1234).unwrap()));
    }

    #[test]
    fn pack_block_chain_get_entry() {
        let mut block = PackBlock::default();
        block[5] =
            PackEntry::new_file("test", StreamOffset(NonZeroU64::new(100).unwrap()), 50, None);

        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        let entry = chain.get(5);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name(), Some("test"));
    }

    #[test]
    fn pack_block_chain_get_entry_across_blocks() {
        let mut block1 = PackBlock::default();
        let mut block2 = PackBlock::default();

        block1[0] =
            PackEntry::new_file("first", StreamOffset(NonZeroU64::new(100).unwrap()), 10, None);
        block2[0] =
            PackEntry::new_file("second", StreamOffset(NonZeroU64::new(200).unwrap()), 20, None);

        let chain = PackBlockChain::from_blocks(vec![
            (BlockOffset(NonZeroU64::new(256).unwrap()), block1),
            (BlockOffset(NonZeroU64::new(3000).unwrap()), block2),
        ]);

        // Entry 0 is in block 0
        assert_eq!(chain.get(0).unwrap().name(), Some("first"));
        // Entry 20 is in block 1 (entry 0 of that block)
        assert_eq!(chain.get(20).unwrap().name(), Some("second"));
    }

    #[test]
    fn pack_block_chain_get_entry_out_of_bounds() {
        let block = PackBlock::default();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        assert!(chain.get(20).is_none());
        assert!(chain.get(100).is_none());
    }

    #[test]
    fn pack_block_chain_get_mut() {
        let block = PackBlock::default();
        let mut chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        {
            let entry = chain.get_mut(3).unwrap();
            *entry = PackEntry::new_file(
                "modified",
                StreamOffset(NonZeroU64::new(999).unwrap()),
                100,
                None,
            );
        }

        assert_eq!(chain.get(3).unwrap().name(), Some("modified"));
    }

    #[test]
    fn pack_block_chain_contains_entry_index() {
        let block = PackBlock::default();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        assert!(chain.contains_entry_index(0));
        assert!(chain.contains_entry_index(19));
        assert!(!chain.contains_entry_index(20));
        assert!(!chain.contains_entry_index(100));
    }

    #[test]
    fn pack_block_chain_stream_offset_for_entry() {
        let block = PackBlock::default();
        let base_offset = BlockOffset(NonZeroU64::new(256).unwrap());
        let chain = PackBlockChain::from_blocks(vec![(base_offset, block)]);

        // Entry 0 should be at offset 256
        let offset0 = chain.stream_offset_for_entry(0);
        assert_eq!(offset0, Some(StreamOffset(NonZeroU64::new(256).unwrap())));

        // Entry 1 should be at offset 256 + 128 = 384
        let offset1 = chain.stream_offset_for_entry(1);
        assert_eq!(offset1, Some(StreamOffset(NonZeroU64::new(384).unwrap())));

        // Entry 5 should be at offset 256 + (5 * 128) = 896
        let offset5 = chain.stream_offset_for_entry(5);
        assert_eq!(offset5, Some(StreamOffset(NonZeroU64::new(896).unwrap())));

        // Out of bounds
        assert!(chain.stream_offset_for_entry(20).is_none());
    }

    #[test]
    fn pack_block_chain_stream_offset_for_entry_multi_block() {
        let block1 = PackBlock::default();
        let block2 = PackBlock::default();
        let offset1 = BlockOffset(NonZeroU64::new(256).unwrap());
        let offset2 = BlockOffset(NonZeroU64::new(5000).unwrap());

        let chain = PackBlockChain::from_blocks(vec![(offset1, block1), (offset2, block2)]);

        // Entry 0 is in block 0 at offset 256
        assert_eq!(
            chain.stream_offset_for_entry(0),
            Some(StreamOffset(NonZeroU64::new(256).unwrap()))
        );

        // Entry 20 is in block 1 (first entry), at offset 5000
        assert_eq!(
            chain.stream_offset_for_entry(20),
            Some(StreamOffset(NonZeroU64::new(5000).unwrap()))
        );

        // Entry 21 is in block 1 at offset 5000 + 128 = 5128
        assert_eq!(
            chain.stream_offset_for_entry(21),
            Some(StreamOffset(NonZeroU64::new(5128).unwrap()))
        );
    }

    #[test]
    fn pack_block_chain_find_block_chain_index_of() {
        let mut block = PackBlock::default();
        let child_chain = ChainOffset(NonZeroU64::new(9999).unwrap());
        block[3] = PackEntry::new_directory("subdir", child_chain, None);

        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        let result = chain.find_block_chain_index_of("subdir");
        assert_eq!(result, Ok(child_chain));
    }

    #[test]
    fn pack_block_chain_find_block_chain_index_of_case_insensitive() {
        let mut block = PackBlock::default();
        let child_chain = ChainOffset(NonZeroU64::new(9999).unwrap());
        block[3] = PackEntry::new_directory("SubDir", child_chain, None);

        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        // Should find regardless of case
        assert_eq!(chain.find_block_chain_index_of("subdir"), Ok(child_chain));
        assert_eq!(chain.find_block_chain_index_of("SUBDIR"), Ok(child_chain));
        assert_eq!(chain.find_block_chain_index_of("SubDir"), Ok(child_chain));
    }

    #[test]
    fn pack_block_chain_find_block_chain_index_of_not_found() {
        let block = PackBlock::default();
        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        let result = chain.find_block_chain_index_of("nonexistent");
        assert_eq!(result, Err(ChainLookupError::NotFound));
    }

    #[test]
    fn pack_block_chain_find_block_chain_index_of_file_not_directory() {
        let mut block = PackBlock::default();
        // Add a file, not a directory
        block[0] =
            PackEntry::new_file("myfile", StreamOffset(NonZeroU64::new(100).unwrap()), 50, None);

        let chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        // Looking for "myfile" as a directory should fail
        let result = chain.find_block_chain_index_of("myfile");
        assert_eq!(result, Err(ChainLookupError::NotFound));
    }

    #[test]
    fn pack_block_chain_last_entry_mut() {
        let block = PackBlock::default();
        let mut chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block)]);

        // Last entry is entry 19
        let last = chain.last_entry_mut();
        *last = PackEntry::new_file("last", StreamOffset(NonZeroU64::new(999).unwrap()), 1, None);

        assert_eq!(chain[19].name(), Some("last"));
    }

    #[test]
    fn pack_block_chain_push_and_link() {
        let mut block1 = PackBlock::default();
        block1[0] =
            PackEntry::new_file("first", StreamOffset(NonZeroU64::new(100).unwrap()), 10, None);

        let mut chain =
            PackBlockChain::from_blocks(vec![(BlockOffset(NonZeroU64::new(256).unwrap()), block1)]);

        let mut block2 = PackBlock::default();
        block2[0] =
            PackEntry::new_file("second", StreamOffset(NonZeroU64::new(200).unwrap()), 20, None);
        let new_offset = BlockOffset(NonZeroU64::new(5000).unwrap());

        chain.push_and_link(new_offset, block2);

        // Chain should now have 40 entries
        assert_eq!(chain.num_entries(), 40);

        // Last entry of first block should have next_block set
        assert_eq!(chain[19].next_block(), Some(new_offset));

        // Can access entries in the second block
        assert_eq!(chain[20].name(), Some("second"));
    }
}
