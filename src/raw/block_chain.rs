use std::io::{Read, Result as IoResult, Write};
use std::ops;

use super::entry::{DirectoryEntry, PackEntry};
use super::{BlockOffset, ChainIndex, EntryOffset};
use crate::constants::*;
use crate::error::{ChainLookupError, ChainLookupResult};
use crate::io::RawIo;

/// A collection of [`PackBlock`]s where each block's next_block field points to
/// the following block in the file. A PackBlockChain is never empty.
pub struct PackBlockChain {
    // (offset, block)
    blocks: Vec<(BlockOffset, PackBlock)>,
}

impl PackBlockChain {
    #[inline]
    pub fn from_blocks(blocks: Vec<(BlockOffset, PackBlock)>) -> Self {
        debug_assert!(!blocks.is_empty());
        PackBlockChain { blocks }
    }

    #[inline]
    pub fn push_and_link(&mut self, offset: BlockOffset, block: PackBlock) {
        self.last_entry_mut().set_next_block(offset);
        self.blocks.push((offset, block));
    }

    #[inline]
    pub fn pop_and_unlink(&mut self) {
        self.blocks.pop();
        assert!(!self.blocks.is_empty());
        self.last_entry_mut().set_next_block(BlockOffset(0));
    }

    /// This blockchains chain index/file offset.
    /// Note: This is the same as its first block
    #[inline]
    pub fn chain_index(&self) -> ChainIndex {
        ChainIndex((self.blocks[0].0).0)
    }

    /// Returns the file offset of the entry at the given idx in this block
    /// chain.
    pub fn stream_offset_for_entry(&self, idx: usize) -> Option<EntryOffset> {
        self.blocks.get(idx / PK2_FILE_BLOCK_ENTRY_COUNT).map(|(BlockOffset(offset), _)| {
            EntryOffset(offset + (PK2_FILE_ENTRY_SIZE * (idx % PK2_FILE_BLOCK_ENTRY_COUNT)) as u64)
        })
    }

    /// Returns the number of PackEntries in this chain.
    #[inline]
    pub fn num_entries(&self) -> usize {
        self.blocks.len() * PK2_FILE_BLOCK_ENTRY_COUNT
    }

    /// Returns the number of PackBlocks in this chain. This is always >= 1.
    #[inline]
    #[allow(clippy::len_without_is_empty)] // a block chain is never empty
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns the last entry of this PackBlockChain.
    #[inline]
    pub fn last_entry(&self) -> &PackEntry {
        &self[self.num_entries() - 1]
    }

    /// Returns the last entry of this PackBlockChain.
    #[inline]
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
            .get(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|(_, block)| block.get(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    /// Get the PackEntry at the specified offset.
    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.blocks
            .get_mut(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|(_, block)| block.get_mut(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    pub fn remove(&mut self, entry: usize) -> Option<PackEntry> {
        self.get_mut(entry).map(PackEntry::clear)
    }

    #[inline]
    pub fn contains_entry_index(&self, entry: usize) -> bool {
        entry < self.num_entries()
    }

    /// Looks up the `directory` name in this [`PackBlockChain`], returning the
    /// offset of the ['PackBlockChain'] corresponding to the directory if
    /// successful.
    pub fn find_block_chain_index_of(&self, directory: &str) -> ChainLookupResult<ChainIndex> {
        self.entries()
            .find(|entry| entry.name_eq_ignore_ascii_case(directory))
            .ok_or(ChainLookupError::NotFound)?
            .as_directory()
            .map(DirectoryEntry::children_position)
            .ok_or(ChainLookupError::ExpectedDirectory)
    }

    pub fn sort(&mut self, scratch: &mut Vec<PackEntry>) {
        use std::cmp::Ordering;
        self.entries_mut()
            .for_each(|entry| scratch.push(std::mem::replace(entry, PackEntry::new_empty(None))));
        scratch.sort_by(|a, b| match (a, b) {
            (PackEntry::Empty(_), PackEntry::Empty(_)) => Ordering::Equal,
            (PackEntry::Empty(_), _) | (PackEntry::File(_), PackEntry::Directory(_)) => {
                Ordering::Greater
            }
            (_, PackEntry::Empty(_)) | (PackEntry::Directory(_), PackEntry::File(_)) => {
                Ordering::Less
            }
            (PackEntry::File(a), PackEntry::File(b)) => a.name().cmp(b.name()),
            (PackEntry::Directory(a), PackEntry::Directory(b)) => a.name().cmp(b.name()),
        });
        self.entries_mut()
            .zip(scratch.drain(..))
            .for_each(|(dst, src)| drop(std::mem::replace(dst, src)));
    }
}

impl ops::Index<usize> for PackBlockChain {
    type Output = PackEntry;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.blocks[idx / PK2_FILE_BLOCK_ENTRY_COUNT].1[idx % PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

impl ops::IndexMut<usize> for PackBlockChain {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.blocks[idx / PK2_FILE_BLOCK_ENTRY_COUNT].1[idx % PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

/// A collection of 20 [`PackEntry`]s.
#[derive(Default)]
pub struct PackBlock {
    entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT],
}

impl PackBlock {
    #[inline]
    pub fn entries(&self) -> std::slice::Iter<PackEntry> {
        self.entries.iter()
    }

    #[inline]
    pub fn entries_mut(&mut self) -> std::slice::IterMut<PackEntry> {
        self.entries.iter_mut()
    }

    #[inline]
    pub fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.entries.get(entry)
    }

    #[inline]
    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.entries.get_mut(entry)
    }
}

impl RawIo for PackBlock {
    fn from_reader<R: Read>(mut r: R) -> IoResult<Self> {
        let mut entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT] = Default::default();
        for entry in &mut entries {
            *entry = PackEntry::from_reader(&mut r)?;
        }
        Ok(PackBlock { entries })
    }

    fn to_writer<W: Write>(&self, mut w: W) -> IoResult<()> {
        self.entries.iter().try_for_each(|entry| entry.to_writer(&mut w))
    }
}

impl ops::Index<usize> for PackBlock {
    type Output = PackEntry;
    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.entries[idx]
    }
}

impl ops::IndexMut<usize> for PackBlock {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.entries[idx]
    }
}
