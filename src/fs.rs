use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::archive::{PackBlockChain, PackEntry, Pk2};
use crate::ChainIndex;
use crate::FILETIME;

pub struct File<'pk2, B = std::fs::File> {
    archive: &'pk2 Pk2<B>,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    seek_pos: u64,
}

impl<'pk2, B> File<'pk2, B> {
    pub(in crate) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        File {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
        }
    }

    pub fn modify_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { modify_time, .. } => *modify_time,
            _ => unreachable!(),
        }
    }

    pub fn access_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { access_time, .. } => *access_time,
            _ => unreachable!(),
        }
    }

    pub fn create_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { create_time, .. } => *create_time,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.entry() {
            PackEntry::File { name, .. } => name,
            _ => unreachable!(),
        }
    }

    pub fn path(&self) -> &str {
        unimplemented!()
    }

    #[inline]
    fn entry(&self) -> &PackEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .expect("invalid file object, this is a bug")
    }

    #[inline]
    fn pos_data_and_size(&self) -> (u64, u32) {
        match *self.entry() {
            PackEntry::File { pos_data, size, .. } => (pos_data, size),
            _ => unreachable!(),
        }
    }
}

impl<B> Seek for File<'_, B> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let size = self.pos_data_and_size().1 as u64;
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.seek_pos = n.min(size);
                return Ok(self.seek_pos);
            }
            SeekFrom::End(n) => (size, n),
            SeekFrom::Current(n) => (self.seek_pos, n),
        };
        let new_pos = base_pos as i64 + offset;
        if new_pos < 0 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ))
        } else {
            self.seek_pos = size.min(new_pos as u64);
            Ok(self.seek_pos)
        }
    }
}

impl<B> Read for File<'_, B>
where
    B: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (pos_data, size) = self.pos_data_and_size();
        let n = {
            let mut file = self.archive.file.file();
            file.seek(SeekFrom::Start(pos_data + self.seek_pos as u64))?;
            let len = buf.len().min((size as u64 - self.seek_pos) as usize);
            file.read(&mut buf[..len])?
        };
        self.seek(SeekFrom::Current(n as i64))?;
        Ok(n)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let len = buf.len();
        let size = self.pos_data_and_size().1 as usize;
        buf.resize(len + size as usize, 0);
        self.read(&mut buf[len..])
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
    pub(in crate) fn new(archive: &'pk2 mut Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        FileMut {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
            data: Vec::new(),
        }
    }

    pub fn modify_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { modify_time, .. } => *modify_time,
            _ => unreachable!(),
        }
    }

    pub fn access_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { access_time, .. } => *access_time,
            _ => unreachable!(),
        }
    }

    pub fn create_time(&self) -> FILETIME {
        match self.entry() {
            PackEntry::File { create_time, .. } => *create_time,
            _ => unreachable!(),
        }
    }

    pub fn modify_time_mut(&mut self) -> &mut FILETIME {
        match self.entry_mut() {
            PackEntry::File { modify_time, .. } => modify_time,
            _ => unreachable!(),
        }
    }

    pub fn access_time_mut(&mut self) -> &mut FILETIME {
        match self.entry_mut() {
            PackEntry::File { access_time, .. } => access_time,
            _ => unreachable!(),
        }
    }

    pub fn create_time_mut(&mut self) -> &mut FILETIME {
        match self.entry_mut() {
            PackEntry::File { create_time, .. } => create_time,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.entry() {
            PackEntry::File { name, .. } => name,
            _ => unreachable!(),
        }
    }

    pub fn path(&self) -> &str {
        unimplemented!()
    }

    #[inline]
    fn entry(&self) -> &PackEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .expect("invalid file object, this is a bug")
    }

    #[inline]
    fn entry_mut(&mut self) -> &mut PackEntry {
        self.archive
            .get_entry_mut(self.chain, self.entry_index)
            .expect("invalid file object, this is a bug")
    }

    #[inline]
    fn pos_data_and_size(&self) -> (u64, u32) {
        match *self.entry() {
            PackEntry::File { pos_data, size, .. } => (pos_data, size),
            _ => unreachable!(),
        }
    }

    fn fetch_data(&mut self) -> io::Result<()> {
        let (pos_data, size) = self.pos_data_and_size();
        self.data.resize(size as usize, 0);
        let mut file = self.archive.file.file();
        file.seek(SeekFrom::Start(pos_data as u64))?;
        file.read_exact(&mut self.data)?;
        Ok(())
    }
}

impl<B> Seek for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let size = self.data.len().max(self.pos_data_and_size().1 as usize) as u64;
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.seek_pos = n.min(size);
                return Ok(self.seek_pos);
            }
            SeekFrom::End(n) => (size, n),
            SeekFrom::Current(n) => (self.seek_pos, n),
        };
        let new_pos = base_pos as i64 + offset;
        if new_pos < 0 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ))
        } else {
            self.seek_pos = size.min(new_pos as u64);
            Ok(self.seek_pos)
        }
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
            let len = buf.len().min((self.data.len() - seek_pos) as usize);
            buf[..len].copy_from_slice(&self.data[seek_pos..seek_pos + len]);
            self.seek(SeekFrom::Current(len as i64))?;
            Ok(len)
        // we dont have the data yet so fetch it then read again
        } else if self.pos_data_and_size().1 > 0 {
            self.fetch_data()?;
            self.read(buf)
        } else {
            Ok(0)
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let len = buf.len();
        let size = self.data.len().max(self.pos_data_and_size().1 as usize);
        buf.resize(len + size as usize, 0);
        self.read(&mut buf[len..])
    }
}

impl<B> Write for FileMut<'_, B>
where
    B: Read + Write + Seek,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let (_, size) = self.pos_data_and_size();
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
                {
                    Some(PackEntry::File { pos_data, size, .. }) => (pos_data, size),
                    _ => panic!("invalid file object, this is a bug"),
                }
            };
            // new unwritten file/more data than what fits, so use a new block
            if self.data.len() > *file_data_size as usize {
                *file_data_pos = self.archive.file.write_new_data_buffer(&self.data)?;
                *file_data_size = self.data.len() as u32;
            // we got data to write that is not bigger than the block we have
            } else {
                self.archive
                    .file
                    .write_data_buffer_at(*file_data_pos, &self.data)?;
                *file_data_size = self.data.len() as u32;
            }
            // update entry
            let entry_offset = self
                .archive
                .get_chain(self.chain)
                .and_then(|chain| chain.file_offset_for_entry(self.entry_index))
                .unwrap();
            self.archive.file.write_entry_at(entry_offset, self.entry())
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

pub struct Directory<'pk2, B = std::fs::File> {
    archive: &'pk2 Pk2<B>,
    chain: ChainIndex,
    entry_index: usize,
}

impl<'pk2, B> Directory<'pk2, B> {
    pub(in crate) fn new(archive: &'pk2 Pk2<B>, chain: ChainIndex, entry_index: usize) -> Self {
        Directory {
            archive,
            chain,
            entry_index,
        }
    }

    #[inline]
    fn entry(&self) -> &PackEntry {
        self.archive
            .get_entry(self.chain, self.entry_index)
            .expect("invalid dir object, this is a bug")
    }

    // returns the chain this folder represents
    #[inline]
    fn dir_chain(&self, chain: ChainIndex) -> &PackBlockChain {
        self.archive
            .get_chain(chain)
            .expect("invalid dir object, this is a bug")
    }

    fn pos_children(&self) -> ChainIndex {
        match self.entry() {
            PackEntry::Directory { pos_children, .. } => *pos_children,
            _ => unreachable!(),
        }
    }

    pub fn name(&self) -> &str {
        match self.entry() {
            PackEntry::Directory { name, .. } => name,
            _ => unreachable!(),
        }
    }

    /// Returns an iterator over all files in this directory.
    pub fn files(&'pk2 self) -> impl Iterator<Item = File<'pk2, B>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::File { .. } => Some(File::new(self.archive, chain, idx)),
                _ => None,
            })
    }

    /// Returns an iterator over all items in this directory excluding `.` and
    /// `..`.
    pub fn entries(&'pk2 self) -> impl Iterator<Item = DirEntry<'pk2, B>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::Directory { name, .. } if name == "." || name == ".." => None,
                PackEntry::File { .. } => Some(DirEntry::File(File::new(self.archive, chain, idx))),
                PackEntry::Directory { .. } => Some(DirEntry::Directory(Directory::new(
                    self.archive,
                    chain,
                    idx,
                ))),
                PackEntry::Empty { .. } => None,
            })
    }
}

pub enum DirEntry<'pk2, B> {
    Directory(Directory<'pk2, B>),
    File(File<'pk2, B>),
}
