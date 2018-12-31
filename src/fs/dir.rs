use std::io::Result;
use std::path::Path;

use crate::archive::PackEntry;
use crate::archive::{PackBlockChain, Pk2};
use crate::fs::file::File;
use crate::PackIndex;
use core::ops;
use std::path::PathBuf;

#[derive(Derivative)]
#[derivative(Copy, Clone, Debug)]
pub struct Directory<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a Pk2,
    parent: Option<PackIndex>,
    chain: u64,
}

impl<'a> Directory<'a> {
    pub fn new(archive: &'a Pk2, chain: u64, parent: Option<PackIndex>) -> Self {
        Directory {
            archive,
            chain,
            parent,
        }
    }

    #[inline]
    fn entry(&self) -> Option<&PackEntry> {
        Some(&self.parent_chain()?[self.parent.unwrap().1])
    }

    #[inline]
    fn chain(&self) -> &PackBlockChain {
        &self.archive.block_mgr.chains[&self.chain]
    }

    #[inline]
    fn parent_chain(&self) -> Option<&PackBlockChain> {
        Some(&self.archive.block_mgr.chains[&self.parent?.0])
    }

    pub fn name(&self) -> &str {
        match self.entry() {
            Some(PackEntry::Folder { name, .. }) => &**name,
            None => "/",
            _ => unreachable!(),
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        match self
            .archive
            .block_mgr
            .resolve_path_to_entry_and_parent(self.chain().blocks[0].offset, path.as_ref())?
        {
            Some((parent, _)) => Ok(File::new(self.archive, parent)),
            None => unreachable!(),
        }
    }

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        match self
            .archive
            .block_mgr
            .resolve_path_to_entry_and_parent(self.chain().blocks[0].offset, path.as_ref())?
        {
            Some((parent, entry)) => Ok(Directory::new(
                self.archive,
                entry.pos_children().unwrap(),
                Some(parent),
            )),
            None => unreachable!(),
        }
    }

    pub fn files(&self) -> impl Iterator<Item = File> {
        self.chain()
            .iter()
            .enumerate()
            .filter_map(move |(idx, entry)| match entry {
                PackEntry::File { .. } => Some(File::new(self.archive, (self.chain, idx))),
                _ => None,
            })
    }

    pub fn directories(&self) -> impl Iterator<Item = Directory> {
        self.chain()
            .iter()
            .enumerate()
            .filter_map(move |(idx, entry)| match entry {
                PackEntry::Folder {
                    pos_children, name, ..
                } => match &**name {
                    "." | ".." => None,
                    _ => Some(Directory::new(
                        self.archive,
                        *pos_children,
                        Some((self.chain, idx)),
                    )),
                },
                _ => None,
            })
    }
}
