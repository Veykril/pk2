use std::io::Result;
use std::path::Path;

use crate::archive::PackEntry;
use crate::archive::{Archive, PackBlockChain};
use crate::fs::file::File;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Directory<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a Archive,
    entry: &'a PackEntry,
    pub block_chain: &'a PackBlockChain,
}

impl<'a> Directory<'a> {
    pub fn new(
        archive: &'a Archive,
        entry: &'a PackEntry,
        block_chain: &'a PackBlockChain,
    ) -> Self {
        Directory {
            archive,
            entry,
            block_chain,
        }
    }

    pub fn name(&self) -> &str {
        match self.entry {
            PackEntry::Folder { name, .. } => name,
            _ => unreachable!(),
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        self.archive
            .resolve_path_to_entry_at(self.block_chain.blocks[0].offset, path.as_ref())
            .map(|(_, _, entry)| File::new(self.archive, entry))
    }

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        self.archive
            .resolve_path_to_entry_at(self.block_chain.blocks[0].offset, path.as_ref())
            .map(|(chain, _, entry)| {
                Directory::new(self.archive, entry, &self.archive.blockchains[&chain])
            })
    }

    pub fn files(&self) -> Files {
        Files {
            archive: self.archive,
            entries: Box::new(self.block_chain.iter()),
        }
    }

    pub fn directories(&self) -> Directories {
        Directories {
            archive: self.archive,
            entries: Box::new(self.block_chain.iter()),
        }
    }
}

pub struct Files<'a> {
    archive: &'a Archive,
    entries: Box<dyn Iterator<Item = &'a PackEntry> + 'a>,
}

impl<'a> Iterator for Files<'a> {
    type Item = File<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let entry @ PackEntry::File { .. } = self.entries.next()? {
                break Some(File::new(self.archive, entry));
            }
        }
    }
}

pub struct Directories<'a> {
    archive: &'a Archive,
    entries: Box<dyn Iterator<Item = &'a PackEntry> + 'a>,
}

impl<'a> Iterator for Directories<'a> {
    type Item = Directory<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.entries.next()?;
            if let PackEntry::Folder { pos_children, .. } = entry {
                break Some(Directory::new(
                    self.archive,
                    entry,
                    &self.archive.blockchains[pos_children],
                ));
            }
        }
    }
}
