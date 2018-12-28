use std::io::Result;
use std::path::Path;

use crate::archive::PackEntry;
use crate::archive::{Archive, PackBlockChain};
use crate::fs::file::File;
use crate::fs::file::FileMut;
use crate::PackIndex;
use core::ops;
use std::path::PathBuf;

#[derive(Derivative)]
#[derivative(Copy, Clone, Debug)]
pub struct Directory<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a Archive,
    parent: Option<PackIndex>,
    chain: u64,
}

impl<'a> Directory<'a> {
    pub fn new(archive: &'a Archive, chain: u64, parent: Option<PackIndex>) -> Self {
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
        &self.archive.blockchains[&self.chain]
    }

    #[inline]
    fn parent_chain(&self) -> Option<&PackBlockChain> {
        Some(&self.archive.blockchains[&self.parent?.0])
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
            .resolve_path_to_entry_and_parent(self.chain().blocks[0].offset, path.as_ref())?
        {
            Some((parent, _)) => Ok(File::new(self.archive, parent)),
            None => unreachable!(),
        }
    }

    pub fn open_dir<P: AsRef<Path>>(&self, path: P) -> Result<Directory> {
        match self
            .archive
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct DirectoryMut<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a mut Archive,
    parent: Option<PackIndex>,
    chain: u64,
}

impl<'a> DirectoryMut<'a> {
    pub fn new(archive: &'a mut Archive, chain: u64, parent: Option<PackIndex>) -> Self {
        DirectoryMut {
            archive,
            chain,
            parent,
        }
    }

    pub fn open_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<FileMut> {
        match self
            .archive
            .resolve_path_to_entry_and_parent(self.chain().blocks[0].offset, path.as_ref())?
        {
            Some((parent, _)) => Ok(FileMut::new(self.archive, parent)),
            None => unreachable!(),
        }
    }

    pub fn open_dir_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<DirectoryMut> {
        match self
            .archive
            .resolve_path_to_entry_and_parent(self.chain().blocks[0].offset, path.as_ref())?
        {
            Some((parent, entry)) => Ok(DirectoryMut::new(
                self.archive,
                entry.pos_children().unwrap(),
                Some(parent),
            )),
            None => unreachable!(),
        }
    }

    // files_mut is not possible to implement safely due to it exposing mutable aliasing
    // gotta rethink this once more
}

impl<'a> ops::Deref for DirectoryMut<'a> {
    type Target = Directory<'a>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self as *const _ as *const Directory) }
    }
}

impl<'a> ops::DerefMut for DirectoryMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self as *mut _ as *mut Directory) }
    }
}
