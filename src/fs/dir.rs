use std::io::Result;
use std::path::Path;

use crate::archive::{PackBlockChain, Archive};
use crate::fs::file::File;

pub struct Directory<'a> {
    archive: &'a Archive,
    block_chain: &'a PackBlockChain,
}

impl<'a> Directory<'a> {
    pub fn new(archive: &'a Archive, block_chain: &'a PackBlockChain) -> Self {
        Directory {
            archive,
            block_chain,
        }
    }

    pub fn open_file<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        self.archive
            .resolve_path_to_entry_at(self.block_chain, path.as_ref())
            .map(|entry| File::new(self.archive, entry))
    }
}
