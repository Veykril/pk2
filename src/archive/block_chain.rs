use std::io::{Read, Result, Write};
use std::ops;

use crate::archive::entry::PackEntry;
use crate::archive::err_not_found;
use crate::constants::PK2_FILE_BLOCK_ENTRY_COUNT;
use std::io;

#[derive(Debug)]
pub struct PackBlockChain {
    pub blocks: Vec<PackBlock>,
}

impl PackBlockChain {
    pub(crate) fn new(blocks: Vec<PackBlock>) -> Self {
        PackBlockChain { blocks }
    }

    pub fn offset(&self) -> u64 {
        self.blocks[0].offset
    }

    pub fn get_file_offset_for_entry(&self, idx: usize) -> Option<u64> {
        Some(
            self.blocks.get(idx / PK2_FILE_BLOCK_ENTRY_COUNT)?.offset
                + (idx % PK2_FILE_BLOCK_ENTRY_COUNT) as u64,
        )
    }

    pub fn find_first_empty_mut(&mut self) -> Option<(usize, &mut PackEntry)> {
        self.iter_mut()
            .enumerate()
            .find(|(_, entry)| entry.is_empty())
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ PackEntry> + '_ {
        self.blocks.iter().flat_map(|blocks| &blocks.entries)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &'_ mut PackEntry> + '_ {
        self.blocks
            .iter_mut()
            .flat_map(|blocks| &mut blocks.entries)
    }

    /// Looks up the `folder` name in the specified [`PackBlockChain`], returning the index of the
    /// ['PackBlockChain'] corresponding to the folder if successful.
    pub fn find_block_chain_index_in(&self, folder: &str) -> Result<u64> {
        for entry in self.iter() {
            return match entry {
                PackEntry::Folder {
                    name, pos_children, ..
                } if name == folder => Ok(*pos_children),
                PackEntry::File { name, .. } if name == folder => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Expected a directory, found a file",
                )),
                _ => continue,
            };
        }
        Err(err_not_found(
            ["Unable to find directory ", folder].join(""),
        ))
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

#[derive(Derivative)]
#[derivative(Debug, Default)]
pub struct PackBlock {
    pub offset: u64,
    #[derivative(Debug = "ignore")]
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
