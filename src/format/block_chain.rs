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
