use std::io::{Read, Write};
use std::ops;

use crate::archive::entry::PackEntry;
use crate::constants::*;
use crate::error::{Error, Pk2Result};
use crate::ChainIndex;

/// A collection of [`PackBlock`]s where each blocks next_block field points to
/// the following block in the file.
pub(crate) struct PackBlockChain {
    // the blocks are boxed to prevent reallocations of the vec from moving them, this would
    // invalidate outstanding references
    blocks: Vec<Box<PackBlock>>,
}

impl PackBlockChain {
    #[inline]
    pub(crate) fn from_blocks(blocks: Vec<Box<PackBlock>>) -> Self {
        debug_assert!(!blocks.is_empty());
        PackBlockChain { blocks }
    }

    #[inline]
    pub(crate) fn push(&mut self, block: PackBlock) {
        self.blocks.push(Box::new(block));
    }

    /// This blockchains chain index/file offset.
    /// Note: This is the same as its first block
    #[inline]
    pub(crate) fn chain_index(&self) -> ChainIndex {
        ChainIndex(self.blocks[0].offset)
    }

    /// Returns the file offset of entry at idx in this block chain.
    pub(crate) fn file_offset_for_entry(&self, idx: usize) -> Option<u64> {
        self.blocks
            .get(idx / PK2_FILE_BLOCK_ENTRY_COUNT)
            .map(|block| {
                block.offset + (PK2_FILE_ENTRY_SIZE * (idx % PK2_FILE_BLOCK_ENTRY_COUNT)) as u64
            })
    }

    /// Fetches the first empty pack entry in this chain, returning its index in
    /// this chain and a mutable reference to it.
    pub(crate) fn find_first_empty_mut(&mut self) -> Option<(usize, &mut PackEntry)> {
        self.entries_mut()
            .enumerate()
            .find(|(_, entry)| entry.is_empty())
    }

    /// An iterator over the entries of this chain.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &PackEntry> {
        self.blocks.iter().flat_map(|block| &block.entries)
    }

    /// An iterator over the entries of this chain.
    pub(crate) fn entries_mut(&mut self) -> impl Iterator<Item = &mut PackEntry> {
        self.blocks.iter_mut().flat_map(|block| &mut block.entries)
    }

    /// Get the PackEntry at the specified offset.
    pub(crate) fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.blocks
            .get(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|block| block.get(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    /// Get the PackEntry at the specified offset.
    pub(crate) fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.blocks
            .get_mut(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|block| block.get_mut(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    /// Looks up the `directory` name in this [`PackBlockChain`], returning the
    /// offset of the ['PackBlockChain'] corresponding to the directory if
    /// successful.
    pub(crate) fn find_block_chain_index_of(&self, directory: &str) -> Pk2Result<ChainIndex> {
        for entry in self.entries() {
            if entry.name() == Some(directory) {
                return match entry {
                    &PackEntry::Directory { pos_children, .. } => Ok(pos_children),
                    PackEntry::File { .. } => Err(Error::ExpectedDirectory),
                    _ => continue,
                };
            }
        }
        Err(Error::NotFound)
    }
}

impl ops::Index<usize> for PackBlockChain {
    type Output = PackEntry;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.blocks[idx / PK2_FILE_BLOCK_ENTRY_COUNT][idx % PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

impl ops::IndexMut<usize> for PackBlockChain {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.blocks[idx / PK2_FILE_BLOCK_ENTRY_COUNT][idx % PK2_FILE_BLOCK_ENTRY_COUNT]
    }
}

/// A collection of 20 [`PackEntry`]s.
#[derive(Default)]
pub(crate) struct PackBlock {
    pub offset: u64,
    entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT],
}

impl PackBlock {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_reader<R: Read>(mut r: R, offset: u64) -> Pk2Result<Self> {
        let mut entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT] = Default::default();
        for entry in &mut entries {
            *entry = PackEntry::from_reader(&mut r)?;
        }
        Ok(PackBlock { offset, entries })
    }

    pub(crate) fn to_writer<W: Write>(&self, mut w: W) -> Pk2Result<()> {
        for entry in &self.entries {
            entry.to_writer(&mut w)?;
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn entries(&self) -> std::slice::Iter<PackEntry> {
        self.entries.iter()
    }

    pub(crate) fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.entries.get(entry)
    }

    pub(crate) fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.entries.get_mut(entry)
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
