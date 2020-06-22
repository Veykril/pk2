#![allow(clippy::match_ref_pats)]
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::SystemTime;

use crate::archive::Pk2;
use crate::error::{Error, Pk2Result};
use crate::raw::block_chain::PackBlockChain;
use crate::raw::entry::{DirectoryEntry, FileEntry, PackEntry};
use crate::raw::{ChainIndex, StreamOffset};

pub struct File<'pk2, B = std::fs::File> {
    archive: &'pk2 Pk2<B>,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    seek_pos: u64,
}

impl<'pk2, B> File<'pk2, B> {
    pub(super) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        File {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
        }
    }

    pub fn modify_time(&self) -> Option<SystemTime> {
        self.entry().modify_time()
    }

    pub fn access_time(&self) -> Option<SystemTime> {
        self.entry().access_time()
    }

    pub fn create_time(&self) -> Option<SystemTime> {
        self.entry().create_time()
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.entry().name()
    }

    #[inline]
    fn entry(&self) -> &FileEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_file)
            .expect("invalid file object, this is a bug")
    }

    #[inline]
    fn remaining_len(&self) -> usize {
        (self.entry().size() as u64 - self.seek_pos) as usize
    }
}

impl<B> Seek for File<'_, B> {
    fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
        let size = self.entry().size() as u64;
        seek_impl(seek, self.seek_pos, size).map(|new_pos| {
            self.seek_pos = new_pos;
            new_pos
        })
    }
}

impl<B> Read for File<'_, B>
where
    B: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos_data = self.entry().pos_data();
        let rem_len = self.remaining_len();
        let len = buf.len().min(rem_len);
        let n = crate::io::read_at(
            &mut *self.archive.file.borrow_mut(),
            pos_data + StreamOffset(self.seek_pos),
            &mut buf[..len],
        )?;
        self.seek(SeekFrom::Current(n as i64))?;
        Ok(n)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let pos_data = self.entry().pos_data();
        let rem_len = self.remaining_len();
        if buf.len() < rem_len {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        } else {
            crate::io::read_at(
                &mut *self.archive.file.borrow_mut(),
                pos_data + StreamOffset(self.seek_pos),
                &mut buf[..rem_len],
            )?;
            self.seek_pos += rem_len as u64;
            Ok(())
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let len = buf.len();
        let rem_len = self.remaining_len();
        buf.resize(len + rem_len, 0);
        self.read_exact(&mut buf[len..]).map(|()| rem_len)
    }
}

pub struct FileMut<'pk2, B = std::fs::File>
where
    B: Read + Write + Seek,
{
    archive: &'pk2 mut Pk2<B>,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    seek_pos: u64,
    data: Vec<u8>,
}

impl<'pk2, B> FileMut<'pk2, B>
where
    B: Read + Write + Seek,
{
    pub(super) fn new(archive: &'pk2 mut Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        FileMut {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
            data: Vec::new(),
        }
    }

    pub fn modify_time(&self) -> Option<SystemTime> {
        self.entry().modify_time.into_systime()
    }

    pub fn access_time(&self) -> Option<SystemTime> {
        self.entry().access_time.into_systime()
    }

    pub fn create_time(&self) -> Option<SystemTime> {
        self.entry().create_time.into_systime()
    }

    pub fn set_modify_time(&mut self, time: SystemTime) {
        self.entry_mut().modify_time = time.into();
    }

    pub fn set_access_time(&mut self, time: SystemTime) {
        self.entry_mut().access_time = time.into();
    }

    pub fn set_create_time(&mut self, time: SystemTime) {
        self.entry_mut().create_time = time.into();
    }

    pub fn copy_file_times<'a, A>(&mut self, other: &File<'a, A>) {
        let this = self.entry_mut();
        let other = other.entry();
        this.modify_time = other.modify_time;
        this.create_time = other.create_time;
        this.access_time = other.access_time;
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.entry().name()
    }

    #[inline]
    fn entry(&self) -> &FileEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_file)
            .expect("invalid file object, this is a bug")
    }

    #[inline]
    fn entry_mut(&mut self) -> &mut FileEntry {
        self.archive
            .get_entry_mut(self.chain, self.entry_index)
            .and_then(PackEntry::as_file_mut)
            .expect("invalid file object, this is a bug")
    }

    fn fetch_data(&mut self) -> io::Result<()> {
        let pos_data = self.entry().pos_data();
        let size = self.entry().size();
        self.data.resize(size as usize, 0);
        crate::io::read_exact_at(
            &mut *self.archive.file.borrow_mut(),
            pos_data,
            &mut self.data,
        )
    }

    #[inline]
    fn remaining_len(&self) -> usize {
        (self.entry().size() as u64 - self.seek_pos) as usize
    }
}

impl<B> Seek for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
        let size = self.data.len().max(self.entry().size() as usize) as u64;
        seek_impl(seek, self.seek_pos, size).map(|new_pos| {
            self.seek_pos = new_pos;
            new_pos
        })
    }
}

impl<B> Read for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            Ok(0)
        // we've got the data in our buffer so read it from there
        } else if !self.data.is_empty() {
            let seek_pos = self.seek_pos as usize;
            let len = buf.len().min(self.data.len() - seek_pos);
            buf[..len].copy_from_slice(&self.data[seek_pos..][..len]);
            self.seek(SeekFrom::Current(len as i64))?;
            Ok(len)
        // we dont have the data yet so fetch it then read again
        } else if self.entry().size() > 0 {
            self.fetch_data()?;
            self.read(buf)
        } else {
            Ok(0)
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let len = buf.len();
        let size = self.data.len().max(self.entry().size() as usize);
        buf.resize(len + size as usize, 0);
        self.read(&mut buf[len..])
    }
}

impl<B> Write for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let size = self.entry().size();
        if !self.data.is_empty() {
            let data_len = self.data.len();
            let seek_pos = self.seek_pos as usize;

            if let Some(slice) = self.data.get_mut(seek_pos..seek_pos + buf.len()) {
                slice.copy_from_slice(buf);
            } else {
                let (copy, extend) = buf.split_at(data_len - seek_pos);
                self.data[seek_pos..].copy_from_slice(copy);
                self.data.extend_from_slice(extend);
            }
            self.seek(SeekFrom::Current(buf.len() as i64))?;
            Ok(buf.len())
        } else if size > 0 {
            self.fetch_data()?;
            self.write(buf)
        } else {
            self.data.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.data.is_empty() {
            let (file_data_pos, file_data_size) = {
                let entry_index = self.entry_index;
                // cant use `entry_mut` since this would borrow self for the whole scope
                match self
                    .archive
                    .block_manager
                    .get_mut(self.chain)
                    .and_then(|chain| chain.get_mut(entry_index))
                    .and_then(PackEntry::as_file_mut)
                {
                    Some(FileEntry { pos_data, size, .. }) => (pos_data, size),
                    None => panic!("invalid file object, this is a bug"),
                }
            };
            // new unwritten file/more data than what fits, so use a new block
            if self.data.len() > *file_data_size as usize {
                *file_data_pos = crate::io::write_new_data_buffer(
                    &mut *self.archive.file.borrow_mut(),
                    &self.data,
                )?;
                *file_data_size = self.data.len() as u32;
            // we got data to write that is not bigger than the block we have
            } else {
                crate::io::write_data_buffer_at(
                    &mut *self.archive.file.borrow_mut(),
                    *file_data_pos,
                    &self.data,
                )?;
                *file_data_size = self.data.len() as u32;
            }
            // update entry
            let entry_offset = self
                .archive
                .get_chain(self.chain)
                .and_then(|chain| chain.file_offset_for_entry(self.entry_index))
                .unwrap();
            self.set_modify_time(SystemTime::now());
            crate::io::write_entry_at(
                self.archive.blowfish.as_ref(),
                &mut *self.archive.file.borrow_mut(),
                entry_offset,
                self.archive
                    .get_entry(self.chain, self.entry_index)
                    .unwrap(),
            )
        } else {
            Ok(())
        }
    }
}

impl<B> Drop for FileMut<'_, B>
where
    B: Write + Read + Seek,
{
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn seek_impl(seek: SeekFrom, seek_pos: u64, size: u64) -> io::Result<u64> {
    let (base_pos, offset) = match seek {
        SeekFrom::Start(n) => {
            return Ok(n.min(size));
        }
        SeekFrom::End(n) => (size, n),
        SeekFrom::Current(n) => (seek_pos, n),
    };
    let new_pos = base_pos as i64 + offset;
    if new_pos < 0 {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative position",
        ))
    } else {
        Ok(size.min(new_pos as u64))
    }
}

pub enum DirEntry<'pk2, B> {
    Directory(Directory<'pk2, B>),
    File(File<'pk2, B>),
}

impl<'pk2, B> DirEntry<'pk2, B> {
    fn from(
        entry: &PackEntry,
        archive: &'pk2 Pk2<B>,
        chain: ChainIndex,
        idx: usize,
    ) -> Option<Self> {
        match entry {
            PackEntry::File(_) => Some(DirEntry::File(File::new(archive, chain, idx))),
            PackEntry::Directory(dir) => {
                if dir.is_normal_link() {
                    Some(DirEntry::Directory(Directory::new(archive, chain, idx)))
                } else {
                    None
                }
            }
            PackEntry::Empty(_) => None,
        }
    }
}

pub struct Directory<'pk2, B = std::fs::File> {
    archive: &'pk2 Pk2<B>,
    chain: ChainIndex,
    entry_index: usize,
}

impl<'pk2, B> Directory<'pk2, B> {
    pub(super) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        Directory {
            archive,
            chain,
            entry_index,
        }
    }

    #[inline]
    fn entry(&self) -> &DirectoryEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_directory)
            .expect("invalid file object, this is a bug")
    }

    // returns the chain this folder represents
    #[inline]
    fn dir_chain(&self, chain: ChainIndex) -> &'pk2 PackBlockChain {
        self.archive
            .get_chain(chain)
            .expect("invalid dir object, this is a bug")
    }

    pub fn name(&self) -> &str {
        self.entry().name()
    }

    pub fn modify_time(&self) -> Option<SystemTime> {
        self.entry().modify_time.into_systime()
    }

    pub fn access_time(&self) -> Option<SystemTime> {
        self.entry().access_time.into_systime()
    }

    pub fn create_time(&self) -> Option<SystemTime> {
        self.entry().create_time.into_systime()
    }

    pub fn open_file(&self, path: impl AsRef<Path>) -> Pk2Result<File<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;
        Pk2::<B>::is_file(entry).map(|_| File::new(self.archive, chain, entry_idx))
    }

    pub fn open_directory(&self, path: impl AsRef<Path>) -> Pk2Result<Directory<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;

        if entry
            .as_directory()
            .map(DirectoryEntry::is_normal_link)
            .unwrap_or(false)
        {
            Ok(Directory::new(self.archive, chain, entry_idx))
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn open(&self, path: impl AsRef<Path>) -> Pk2Result<DirEntry<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;
        DirEntry::from(entry, self.archive, chain, entry_idx).ok_or(Error::NotFound)
    }

    /// Returns an iterator over all files in this directory.
    pub fn files(&self) -> impl Iterator<Item = File<'pk2, B>> {
        let chain = self.entry().children_position();
        let archive = self.archive;
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| entry.as_file().map(|_| File::new(archive, chain, idx)))
    }

    /// Returns an iterator over all items in this directory excluding `.` and
    /// `..`.
    pub fn entries(&self) -> impl Iterator<Item = DirEntry<'pk2, B>> {
        let chain = self.entry().children_position();
        let archive = self.archive;
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| DirEntry::from(entry, archive, chain, idx))
    }
}
