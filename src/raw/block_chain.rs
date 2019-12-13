use std::io::{Read, Result as IoResult, Seek, Write};
use std::ops;

use super::entry::{DirectoryEntry, PackEntry};
use super::ChainIndex;
use crate::constants::*;
use crate::error::{Error, Pk2Result};
use crate::io::{file_len, write_block, write_entry_at, RawIo};
use crate::Blowfish;

pub type OffsetBlock = (u64, PackBlock);

/// A collection of [`PackBlock`]s where each blocks next_block field points to
/// the following block in the file. A PackBlockChain is never empty.
pub struct PackBlockChain {
    // (offset, block)
    blocks: Vec<(u64, PackBlock)>,
}

#[allow(clippy::len_without_is_empty)]
impl PackBlockChain {
    #[inline]
    pub fn from_blocks(blocks: Vec<(u64, PackBlock)>) -> Self {
        debug_assert!(!blocks.is_empty());
        PackBlockChain { blocks }
    }

    #[inline]
    pub(crate) fn push(&mut self, offset: u64, block: PackBlock) {
        self.blocks.push((offset, block));
    }

    /// This blockchains chain index/file offset.
    /// Note: This is the same as its first block
    #[inline]
    pub(crate) fn chain_index(&self) -> ChainIndex {
        ChainIndex(self.blocks[0].0)
    }

    /// Returns the file offset of the entry at the given idx in this block
    /// chain.
    pub(crate) fn file_offset_for_entry(&self, idx: usize) -> Option<u64> {
        self.blocks
            .get(idx / PK2_FILE_BLOCK_ENTRY_COUNT)
            .map(|(offset, _)| {
                offset + (PK2_FILE_ENTRY_SIZE * (idx % PK2_FILE_BLOCK_ENTRY_COUNT)) as u64
            })
    }

    /// Returns the number of PackEntries in this chain.
    #[inline]
    pub fn num_entries(&self) -> usize {
        self.blocks.len() * PK2_FILE_BLOCK_ENTRY_COUNT
    }

    /// Returns the number of PackBlocks in this chain. This is always >= 1.
    #[inline]
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// An iterator over the entries of this chain.
    pub fn entries(&self) -> impl Iterator<Item = &PackEntry> {
        self.blocks.iter().flat_map(|block| &block.1.entries)
    }

    /// An iterator over the entries of this chain.
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut PackEntry> {
        self.blocks
            .iter_mut()
            .flat_map(|block| &mut block.1.entries)
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

    /// Looks up the `directory` name in this [`PackBlockChain`], returning the
    /// offset of the ['PackBlockChain'] corresponding to the directory if
    /// successful.
    pub fn find_block_chain_index_of(&self, directory: &str) -> Pk2Result<ChainIndex> {
        self.entries()
            .find(|entry| entry.name() == Some(directory))
            .ok_or(Error::NotFound)
            .and_then(|entry| {
                entry
                    .as_directory()
                    .map(DirectoryEntry::children_position)
                    .ok_or(Error::ExpectedDirectory)
            })
    }

    /// Creates a new block in the file, appends it to this chain and returns
    /// the entry index of the first entry of the new block relative to the
    /// chain.
    pub(crate) fn create_new_block<F: Write + Seek>(
        &mut self,
        bf: Option<&Blowfish>,
        mut file: F,
    ) -> Pk2Result<usize> {
        let new_block_offset = file_len(&mut file)?;
        let block = PackBlock::default();
        write_block(bf, &mut file, new_block_offset, &block)?;
        let last_idx = self.num_entries() - 1;
        self[last_idx].set_next_block(new_block_offset);
        write_entry_at(
            bf,
            &mut file,
            self.file_offset_for_entry(last_idx).unwrap(),
            &self[last_idx],
        )?;
        self.push(new_block_offset, block);
        Ok(last_idx + 1)
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
    fn from_reader<R: Read>(mut r: R) -> Pk2Result<Self> {
        let mut entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT] = Default::default();
        for entry in &mut entries {
            *entry = PackEntry::from_reader(&mut r)?;
        }
        Ok(PackBlock { entries })
    }

    fn to_writer<W: Write>(&self, mut w: W) -> IoResult<()> {
        self.entries
            .iter()
            .map(|entry| entry.to_writer(&mut w))
            .collect()
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
