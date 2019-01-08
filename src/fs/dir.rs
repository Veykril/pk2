use std::io::Result;
use std::path::Path;

use crate::archive::{PackBlockChain, PackEntry, Pk2};
use crate::fs::file::File;

#[derive(Copy, Clone)]
pub struct Directory<'a> {
    archive: &'a Pk2,
    entry: Option<&'a PackEntry>,
    block_chain: &'a PackBlockChain,
}

impl<'a> Directory<'a> {
    pub(in crate) fn new(
        archive: &'a Pk2,
        block_chain: &'a PackBlockChain,
        entry: Option<&'a PackEntry>,
    ) -> Self {
        Directory {
            archive,
            entry,
            block_chain,
        }
    }

    pub fn name(&self) -> &str {
        match self.entry {
            Some(PackEntry::Directory { name, .. }) => &**name,
            None => "/",
            _ => unreachable!(),
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        match self
            .archive
            .block_mgr
            .resolve_path_to_entry_and_parent(self.block_chain.offset(), path.as_ref())?
        {
            Some((_, _, e)) => Ok(File::new(self.archive, e)),
            None => unreachable!(),
        }
    }

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        let (parent, _, entry) = self
            .archive
            .block_mgr
            .resolve_path_to_entry_and_parent(self.block_chain.offset(), path.as_ref())?
            .unwrap();
        Ok(Directory::new(self.archive, parent, Some(entry)))
    }

    pub fn files(&self) -> impl Iterator<Item = File> {
        self.block_chain
            .iter()
            .filter_map(move |entry| match entry {
                PackEntry::File { .. } => Some(File::new(self.archive, entry)),
                _ => None,
            })
    }

    pub fn directories(&self) -> impl Iterator<Item = Directory> {
        self.block_chain
            .iter()
            .filter_map(move |entry| match entry {
                PackEntry::Directory {
                    ref pos_children,
                    name,
                    ..
                } => match &**name {
                    "." | ".." => None,
                    _ => Some(Directory::new(
                        self.archive,
                        &self.archive.block_mgr.chains[pos_children],
                        Some(entry),
                    )),
                },
                _ => None,
            })
    }
}
