use std::io::{self, Read, Result, Write};
use std::ops;

use crate::archive::entry::PackEntry;
use crate::archive::err_not_found;
use crate::constants::*;
use crate::ChainIndex;

pub struct PackBlockChain {
    // the blocks are boxed to prevent reallocations of the vec from moving them, this would invalidate outstanding references
    blocks: Vec<Box<PackBlock>>,
}

impl PackBlockChain {
    #[inline]
    pub fn from_blocks(blocks: Vec<Box<PackBlock>>) -> Self {
        debug_assert!(!blocks.is_empty());
        PackBlockChain { blocks }
    }

    #[inline]
    pub fn push(&mut self, block: PackBlock) {
        self.blocks.push(Box::new(block));
    }

    /// This blockchains chain index/file offset.
    /// Note: This is the same as its first block
    #[inline]
    pub fn chain_index(&self) -> ChainIndex {
        self.blocks[0].offset
    }

    /// Returns the file offset of entry at idx in this block chain.
    pub fn file_offset_for_entry(&self, idx: usize) -> Option<ChainIndex> {
        self.blocks
            .get(idx / PK2_FILE_BLOCK_ENTRY_COUNT)
            .map(|block| {
                block.offset
                    + (PK2_FILE_ENTRY_SIZE * (idx % PK2_FILE_BLOCK_ENTRY_COUNT)) as ChainIndex
            })
    }

    /// Fetches the first empty pack entry in this chain
    pub fn find_first_empty_mut(&mut self) -> Option<(usize, &mut PackEntry)> {
        self.entries_mut()
            .enumerate()
            .find(|(_, entry)| entry.is_empty())
    }

    pub fn entries(&self) -> impl Iterator<Item = &PackEntry> {
        self.blocks.iter().flat_map(|block| &block.entries)
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut PackEntry> {
        self.blocks.iter_mut().flat_map(|block| &mut block.entries)
    }

    pub fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.blocks
            .get(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|block| block.get(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
        self.blocks
            .get_mut(entry / PK2_FILE_BLOCK_ENTRY_COUNT)
            .and_then(|block| block.get_mut(entry % PK2_FILE_BLOCK_ENTRY_COUNT))
    }

    /// Looks up the `directory` name in this [`PackBlockChain`], returning the offset of the
    /// ['PackBlockChain'] corresponding to the directory if successful.
    pub fn find_block_chain_index_of(&self, directory: &str) -> Result<ChainIndex> {
        for entry in self.entries() {
            if entry.name() == Some(directory) {
                return match entry {
                    &PackEntry::Directory { pos_children, .. } => Ok(pos_children),
                    PackEntry::File { .. } => Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "found file where directory was expected",
                    )),
                    _ => continue,
                };
            }
        }
        Err(err_not_found(["directory not found: ", directory].concat()))
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

#[derive(Default)]
pub struct PackBlock {
    pub offset: ChainIndex,
    pub entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT],
}

impl PackBlock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_reader<R: Read>(mut r: R, offset: ChainIndex) -> Result<Self> {
        let mut entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT] = Default::default();
        for entry in &mut entries {
            *entry = PackEntry::from_reader(&mut r)?;
        }
        Ok(PackBlock { offset, entries })
    }

    pub fn to_writer<W: Write>(&self, mut w: W) -> Result<()> {
        for entry in &self.entries {
            entry.to_writer(&mut w)?;
        }
        Ok(())
    }

    pub fn get(&self, entry: usize) -> Option<&PackEntry> {
        self.entries.get(entry)
    }

    pub fn get_mut(&mut self, entry: usize) -> Option<&mut PackEntry> {
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
