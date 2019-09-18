use std::io::{self, Read, Result, Seek, SeekFrom, Write};

use crate::archive::{PackEntry, Pk2};
use crate::ChainIndex;
use crate::FILETIME;

pub struct File<'pk2> {
    archive: &'pk2 Pk2,
    // the chain this file resides in
    chain: ChainIndex,
    // the index of this file in the chain
    entry_index: usize,
    seek_pos: usize,
    // is some only if this file is writeable
    // in the case it is a non-empty Vec it will have copied the actual data inside of the archive
    data: Option<Vec<u8>>,
}

impl<'pk2> File<'pk2> {
    pub(in crate) fn new_write(
        archive: &'pk2 Pk2,
        chain: ChainIndex,
        entry_index: usize,
    ) -> Result<Self> {
        archive.borrow_file_mut(chain, entry_index)?;
        Ok(File {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
            data: Some(Vec::new()),
        })
    }

    pub(in crate) fn new_read(
        archive: &'pk2 Pk2,
        chain: ChainIndex,
        entry_index: usize,
    ) -> Result<Self> {
        archive.borrow_file(chain, entry_index)?;
        Ok(File {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
            data: None,
        })
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
}

impl Seek for File<'_> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let size = self
            .data
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0)
            .max(self.pos_data_and_size().1 as usize);
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.seek_pos = (n as usize).min(size);
                return Ok(self.seek_pos as u64);
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
            self.seek_pos = (new_pos as usize).min(size);
            Ok(self.seek_pos as u64)
        }
    }
}

impl Read for File<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            Ok(0)
        // we've got the data in our buffer so read it from there
        } else if let Some(data) = self.data.as_ref() {
            let len = (data.len() - self.seek_pos).min(buf.len());
            buf[..len].copy_from_slice(&data[self.seek_pos..self.seek_pos + len]);
            self.seek(SeekFrom::Current(len as i64))?;
            Ok(len)
        } else {
            let (pos_data, size) = self.pos_data_and_size();
            let n = {
                let mut file = self.archive.file.file();
                file.seek(SeekFrom::Start(pos_data + self.seek_pos as u64))?;
                let len = (size as usize - self.seek_pos).min(buf.len());
                file.read(&mut buf[..len])?
            };
            self.seek(SeekFrom::Current(n as i64))?;
            Ok(n)
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        let len = buf.len();
        let size = self
            .data
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0)
            .max(self.pos_data_and_size().1 as usize);
        buf.resize(len + size as usize, 0);
        self.read(&mut buf[len..])
    }
}

impl Write for File<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let (pos_data, size) = self.pos_data_and_size();
        if let Some(data) = self.data.as_mut() {
            // our buffer is empty so read out the actual file contents and place them into
            // our buffer so they wont get lost
            if data.is_empty() && size > 0 {
                let mut file = self.archive.file.file();
                file.seek(SeekFrom::Start(pos_data))?;
                (&mut *file).take(size.into()).read_to_end(data)?;
            }
            let data_len = data.len();
            if self.seek_pos + buf.len() <= data_len {
                data[self.seek_pos..self.seek_pos + buf.len()].copy_from_slice(buf);
            } else {
                data[self.seek_pos..].copy_from_slice(&buf[..data_len - self.seek_pos]);
                data.extend_from_slice(&buf[data_len - self.seek_pos..]);
            }
            self.seek(SeekFrom::Current(buf.len() as i64))?;
            Ok(buf.len())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file was opened in read-only mode",
            ))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(data) = self.data.as_ref() {
            let (file_data_pos, file_data_size) =
                // cant use `entry_mut` since this would borrow all of self for the whole scope
                match self.archive.get_entry_mut(self.chain, self.entry_index) {
                    Some(PackEntry::File { pos_data, size, .. }) => (pos_data, size),
                    _ => panic!("invalid file object, this is a bug"),
                };
            // new unwritten file/more data than what fits, so use a new block
            if data.len() > *file_data_size as usize {
                *file_data_pos = self.archive.file.write_new_data_buffer(data)?;
                *file_data_size = data.len() as u32;
            // we got data to write that is not bigger than the block we have
            } else if !data.is_empty() {
                self.archive
                    .file
                    .write_data_buffer_at(*file_data_pos, data)?;
                *file_data_size = data.len() as u32;
            } else {
                // exit early, dont need to update the entry
                return Ok(());
            }
            // update entry
            let entry_offset = self
                .archive
                .get_chain(self.chain)
                .and_then(|chain| chain.file_offset_for_entry(self.entry_index))
                .unwrap();
            self.archive
                .file
                .write_entry_at(entry_offset, self.entry())?;
        }
        Ok(())
    }
}

impl Drop for File<'_> {
    fn drop(&mut self) {
        let _ = self.flush();
        self.archive.drop_borrow(self.chain, self.entry_index);
    }
}

pub struct Directory<'pk2> {
    archive: &'pk2 Pk2,
    chain: ChainIndex,
    entry_index: usize,
}

use crate::archive::PackBlockChain;
impl<'pk2> Directory<'pk2> {
    pub(in crate) fn new(archive: &'pk2 Pk2, chain: ChainIndex, entry_index: usize) -> Self {
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
            .expect("folder pointed to an invalid chain, this is a bug")
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

    pub fn files(&'pk2 self) -> impl Iterator<Item = Result<File<'pk2>>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::File { .. } => Some(File::new_read(self.archive, chain, idx)),
                _ => None,
            })
    }

    pub fn files_mut(&'pk2 self) -> impl Iterator<Item = Result<File<'pk2>>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::File { .. } => Some(File::new_write(self.archive, chain, idx)),
                _ => None,
            })
    }

    /// Returns an iterator over all file items in this directory.
    pub fn entries(&'pk2 self) -> impl Iterator<Item = Result<DirEntry<'pk2>>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::Directory { name, .. } if name == "." || name == ".." => None,
                PackEntry::File { .. } => {
                    Some(File::new_read(self.archive, chain, idx).map(DirEntry::File))
                }
                PackEntry::Directory { .. } => Some(Ok(DirEntry::Directory(Directory::new(
                    self.archive,
                    chain,
                    idx,
                )))),
                PackEntry::Empty { .. } => None,
            })
    }

    /// Returns an iterator over all file items in this directory with files
    /// being opened as writable.
    pub fn entries_mut(&'pk2 self) -> impl Iterator<Item = Result<DirEntry<'pk2>>> {
        let chain = self.pos_children();
        self.dir_chain(chain)
            .entries()
            .enumerate()
            .flat_map(move |(idx, entry)| match entry {
                PackEntry::Directory { name, .. } if name == "." || name == ".." => None,
                PackEntry::File { .. } => {
                    Some(File::new_write(self.archive, chain, idx).map(DirEntry::File))
                }
                PackEntry::Directory { .. } => Some(Ok(DirEntry::Directory(Directory::new(
                    self.archive,
                    chain,
                    idx,
                )))),
                PackEntry::Empty { .. } => None,
            })
    }
}

pub enum DirEntry<'pk2> {
    Directory(Directory<'pk2>),
    File(File<'pk2>),
}
