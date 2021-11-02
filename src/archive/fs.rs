//! File structs representing file entries inside a pk2 archive.
use std::hash::Hash;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::mem;
use std::path::Path;
use std::time::SystemTime;

use crate::archive::Pk2;
use crate::error::{ChainLookupError, ChainLookupResult};
use crate::raw::block_chain::PackBlockChain;
use crate::raw::entry::{DirectoryEntry, FileEntry, PackEntry};
use crate::raw::{ChainIndex, StreamOffset};

/// A read-only file handle.
pub struct File<'pk2, B = std::fs::File> {
    archive: &'pk2 Pk2<B>,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    seek_pos: u64,
}

impl<'pk2, B> Copy for File<'pk2, B> {}
impl<'pk2, B> Clone for File<'pk2, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'pk2, B> File<'pk2, B> {
    pub(super) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        File { archive, chain, entry_index, seek_pos: 0 }
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

    pub fn size(&self) -> u32 {
        self.entry().size
    }

    pub fn name(&self) -> &'pk2 str {
        self.entry().name()
    }

    fn entry(&self) -> &'pk2 FileEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_file)
            .expect("invalid file object")
    }

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
            &mut *self.archive.stream.borrow_mut(),
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
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "failed to fill whole buffer"))
        } else {
            crate::io::read_at(
                &mut *self.archive.stream.borrow_mut(),
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

/// A write-able file handle.
pub struct FileMut<'pk2, B = std::fs::File>
where
    B: Read + Write + Seek,
{
    archive: &'pk2 mut Pk2<B>,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    data: Cursor<Vec<u8>>,
}

impl<'pk2, B> FileMut<'pk2, B>
where
    B: Read + Write + Seek,
{
    pub(super) fn new(archive: &'pk2 mut Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        FileMut { archive, chain, entry_index, data: Cursor::new(Vec::new()) }
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

    pub fn size(&self) -> u32 {
        self.entry().size
    }

    pub fn flush_drop(mut self) -> io::Result<()> {
        let res = self.flush();
        std::mem::forget(self);
        res
    }

    pub fn name(&self) -> &str {
        self.entry().name()
    }

    fn entry(&self) -> &FileEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_file)
            .expect("invalid file object")
    }

    fn entry_mut(&mut self) -> &mut FileEntry {
        self.archive
            .get_entry_mut(self.chain, self.entry_index)
            .and_then(PackEntry::as_file_mut)
            .expect("invalid file object")
    }

    fn fetch_data(&mut self) -> io::Result<()> {
        let pos_data = self.entry().pos_data();
        let size = self.entry().size();
        self.data.get_mut().resize(size as usize, 0);
        crate::io::read_exact_at(
            &mut *self.archive.stream.borrow_mut(),
            pos_data,
            self.data.get_mut(),
        )
    }

    fn try_fetch_data(&mut self) -> io::Result<()> {
        if self.data.get_ref().is_empty() && self.entry().size() > 0 {
            self.fetch_data()
        } else {
            Ok(())
        }
    }
}

impl<B> Seek for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
        let size = self.data.get_ref().len().max(self.entry().size() as usize) as u64;
        seek_impl(seek, self.data.position(), size).map(|new_pos| {
            self.data.set_position(new_pos);
            new_pos
        })
    }
}

impl<B> Read for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.try_fetch_data()?;
        self.data.read(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.try_fetch_data()?;
        self.data.read_exact(buf)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let len = buf.len();
        let size = self.data.get_ref().len().max(self.entry().size() as usize);
        buf.resize(len + size as usize, 0);
        self.read_exact(&mut buf[len..]).map(|()| size)
    }
}

impl<B> Write for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.try_fetch_data()?;
        let len = self.data.get_ref().len();
        match len.checked_add(buf.len()).map(|new_len| new_len.checked_sub(u32::MAX as usize)) {
            // data + buf < u32::MAX
            Some(None | Some(0)) => self.data.write(buf),
            // data + buf > u32::MAX, truncate buf
            Some(Some(slice_overflow)) => self.data.write(&buf[..buf.len() - slice_overflow]),
            // data + buf overflows usize::MAX
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.data.get_ref().is_empty() {
            return Ok(()); // nothing to write
        }
        self.set_modify_time(SystemTime::now());
        let chain = self.archive.block_manager.get_mut(self.chain).expect("invalid chain");
        let entry_offset = chain.stream_offset_for_entry(self.entry_index).expect("invalid entry");

        let entry = chain.get_mut(self.entry_index).expect("invalid entry");
        let fentry = entry.as_file_mut().expect("invalid file object, this is a bug");

        let stream = &mut *self.archive.stream.borrow_mut();
        let data = &self.data.get_ref()[..];
        debug_assert!(data.len() <= !0u32 as usize);
        let data_len = data.len() as u32;
        // new unwritten file/more data than what fits, so use a new block
        if data_len > fentry.size {
            // FIXME reuse previous buffer somehow?
            fentry.pos_data = crate::io::append_data(&mut *stream, data)?;
        // data fits into the previous buffer space
        } else {
            crate::io::write_data_at(&mut *stream, fentry.pos_data, data)?;
        }
        fentry.size = data_len;

        crate::io::write_entry_at(self.archive.blowfish.as_ref(), stream, entry_offset, entry)
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
            return Ok(n);
        }
        SeekFrom::End(n) => (size, n),
        SeekFrom::Current(n) => (seek_pos, n),
    };
    let new_pos = if offset >= 0 {
        base_pos.checked_add(offset as u64)
    } else {
        base_pos.checked_sub((offset.wrapping_neg()) as u64)
    };
    match new_pos {
        Some(n) => Ok(n),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative or overflowing position",
        )),
    }
}

pub enum DirEntry<'pk2, B> {
    Directory(Directory<'pk2, B>),
    File(File<'pk2, B>),
}

impl<'pk2, B> Copy for DirEntry<'pk2, B> {}
impl<'pk2, B> Clone for DirEntry<'pk2, B> {
    fn clone(&self) -> Self {
        *self
    }
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

impl<'pk2, B> Copy for Directory<'pk2, B> {}
impl<'pk2, B> Clone for Directory<'pk2, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'pk2, B> Directory<'pk2, B> {
    pub(super) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        Directory { archive, chain, entry_index }
    }

    fn entry(&self) -> &'pk2 DirectoryEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .and_then(PackEntry::as_directory)
            .expect("invalid file object")
    }

    // returns the chain this folder represents
    fn dir_chain(&self, chain: ChainIndex) -> &'pk2 PackBlockChain {
        self.archive.get_chain(chain).expect("invalid dir object")
    }

    pub fn name(&self) -> &'pk2 str {
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

    pub fn open_file(&self, path: impl AsRef<Path>) -> ChainLookupResult<File<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;
        Pk2::<B>::is_file(entry).map(|_| File::new(self.archive, chain, entry_idx))
    }

    pub fn open_directory(&self, path: impl AsRef<Path>) -> ChainLookupResult<Directory<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;

        if entry.as_directory().map(DirectoryEntry::is_normal_link).unwrap_or(false) {
            Ok(Directory::new(self.archive, chain, entry_idx))
        } else {
            Err(ChainLookupError::NotFound)
        }
    }

    pub fn open(&self, path: impl AsRef<Path>) -> ChainLookupResult<DirEntry<'pk2, B>> {
        let (chain, entry_idx, entry) = self
            .archive
            .block_manager
            .resolve_path_to_entry_and_parent(self.chain, path.as_ref())?;
        DirEntry::from(entry, self.archive, chain, entry_idx).ok_or(ChainLookupError::NotFound)
    }

    /// Invokes cb on every file in this directory and its children
    /// The callback gets invoked with its relative path to `base` and the file object.
    // Todo, replace this with a file_paths iterator once generators are stable
    pub fn for_each_file(
        &self,
        mut cb: impl FnMut(&Path, File<B>) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut path = std::path::PathBuf::new();
        let mut stack = vec![*self];
        let mut first_iteration = true;

        while let Some(dir) = stack.pop() {
            if !mem::take(&mut first_iteration) {
                path.push(dir.name());
            };
            let mut dir_done = true;
            for entry in dir.entries() {
                match entry {
                    DirEntry::Directory(dir) => {
                        stack.push(dir);
                        dir_done = false;
                    }
                    DirEntry::File(file) => {
                        path.push(file.name());
                        cb(&path, file)?;
                        path.pop();
                    }
                }
            }
            if dir_done {
                path.pop();
            }
        }
        Ok(())
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

impl<B> Hash for Directory<'_, B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.chain.0);
        state.write_usize(self.entry_index);
    }
}

impl<B: Read> Hash for File<'_, B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.chain.0);
        state.write_usize(self.entry_index);
    }
}

impl<B: Read + Write + Seek> Hash for FileMut<'_, B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.chain.0);
        state.write_usize(self.entry_index);
    }
}
