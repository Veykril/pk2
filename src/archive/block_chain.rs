use std::io::{Read, Result, Write};
use std::ops;

use crate::archive::entry::PackEntry;
use crate::constants::PK2_FILE_BLOCK_ENTRY_COUNT;

#[derive(Debug)]
pub struct PackBlockChain {
    pub blocks: Vec<PackBlock>,
}

impl PackBlockChain {
    pub(crate) fn new(blocks: Vec<PackBlock>) -> Self {
        PackBlockChain { blocks }
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

impl<'a> IntoIterator for &'a PackBlockChain {
    type Item = &'a PackEntry;
    type IntoIter = PackBlockChainIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        PackBlockChainIter {
            block_chain_index: 0,
            block_index: 0,
            blocks: &self.blocks,
        }
    }
}

pub struct PackBlockChainIter<'a> {
    block_chain_index: usize,
    block_index: usize,
    blocks: &'a [PackBlock],
}

impl<'a> Iterator for PackBlockChainIter<'a> {
    type Item = &'a PackEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.block_chain_index < self.blocks.len() {
            let block = &self.blocks[self.block_chain_index];
            if self.block_index < block.entries.len() {
                self.block_index += 1;
                Some(&block[self.block_index - 1])
            } else {
                self.block_index = 0;
                self.block_chain_index += 1;
                None
            }
        } else {
            None
        }
    }
}

impl<'a> std::iter::FusedIterator for PackBlockChainIter<'a> {}

#[derive(Debug)]
pub struct PackBlock {
    pub offset: u64,
    pub entries: [PackEntry; PK2_FILE_BLOCK_ENTRY_COUNT],
}

impl PackBlock {
    pub(crate) fn from_reader<R: Read>(mut r: R, offset: u64) -> Result<Self> {
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
