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

    pub fn iter(&self) -> impl Iterator<Item = &'_ PackEntry> + '_ {
        self.blocks.iter().flat_map(|blocks| &blocks.entries)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &'_ mut PackEntry> + '_ {
        self.blocks
            .iter_mut()
            .flat_map(|blocks| &mut blocks.entries)
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
