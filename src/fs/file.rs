use std::io::{self, Read, Result, Seek, SeekFrom, Write};
use std::path::Path;

use crate::archive::{Archive, PackEntry};

#[derive(Derivative)]
#[derivative(Debug)]
#[derive(Copy, Clone)]
pub struct File<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a Archive,
    entry: &'a PackEntry,
    pos: u64, //internal seek
}

impl<'a> File<'a> {
    pub fn open<P: AsRef<Path>>(archive: &'a Archive, path: P) -> Result<Self> {
        archive.open_file(path)
    }

    pub(crate) fn new(archive: &'a Archive, entry: &'a PackEntry) -> Self {
        match entry {
            PackEntry::File { .. } => File {
                archive,
                entry,
                pos: 0,
            },
            _ => panic!("tried constructing file object with wrong PackEntry"),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.entry {
            PackEntry::File { name, .. } => name,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn pos_data_and_size(&self) -> (u64, u32) {
        match *self.entry {
            PackEntry::File { pos_data, size, .. } => (pos_data, size),
            _ => unreachable!(),
        }
    }
}

impl Read for File<'_> {
    // todo read_to_end calls this over and over again, making the seeking part quite inefficient better implement read_to_end ourselves
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            Ok(0)
        } else {
            let (pos_data, size) = self.pos_data_and_size();
            let n = match self.archive.file.borrow_mut() {
                mut file => {
                    file.seek(SeekFrom::Start(pos_data + self.pos))?;
                    let len = (size as usize - self.pos as usize).min(buf.len());
                    file.read(&mut buf[..len])?
                }
            };
            self.seek(SeekFrom::Current(n as i64)).unwrap();
            Ok(n)
        }
    }
}

impl Seek for File<'_> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let size = self.pos_data_and_size().1 as u64;
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.pos = n.min(size);
                return Ok(n);
            }
            SeekFrom::End(n) => (size, n),
            SeekFrom::Current(n) => (self.pos, n),
        };
        let new_pos = base_pos as i64 + offset;
        if new_pos < 0 || new_pos > size as i64 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or size overflowing position",
            ))
        } else {
            self.pos = new_pos as u64;
            Ok(self.pos)
        }
    }
}

pub struct FileMut<'a> {
    archive: &'a mut Archive,
    /// we cannot have access to a `&'a mut PackEntry` here because we would mutably alias it with the archive refernce then
    map_index: u64,
    block_chain_index: usize,
    pos: u64, //internal seek
}

impl<'a> FileMut<'a> {
    pub fn open<P: AsRef<Path>>(archive: &'a mut Archive, path: P) -> Result<Self> {
        archive.open_file_mut(path)
    }

    pub(crate) fn new(archive: &'a mut Archive, map_index: u64, block_chain_index: usize) -> Self {
        FileMut {
            archive,
            map_index,
            block_chain_index,
            pos: 0,
        }
    }

    fn to_immutable(&self) -> File {
        File {
            archive: self.archive,
            entry: self.entry(),
            pos: self.pos,
        }
    }

    fn entry(&self) -> &PackEntry {
        &self.archive.blockchains[&self.map_index][self.block_chain_index]
    }

    fn entry_mut(&mut self) -> &mut PackEntry {
        &mut self.archive.blockchains.get_mut(&self.map_index).unwrap()[self.block_chain_index]
    }
}

impl Read for FileMut<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut this = self.to_immutable();
        let n = this.read(buf)?;
        self.pos = this.pos;
        Ok(n)
    }
}

impl Seek for FileMut<'_> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        self.pos = self.to_immutable().seek(pos)?;
        Ok(self.pos)
    }
}

impl Write for FileMut<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            Ok(0)
        } else {
            let (pos_data, size) = match *self.entry() {
                PackEntry::File { pos_data, size, .. } => (pos_data, size),
                _ => unreachable!(),
            };
            let n = match self.archive.file.borrow_mut() {
                mut file => {
                    file.seek(SeekFrom::Start(pos_data + self.pos))?;
                    let len = (size as usize - self.pos as usize).min(buf.len());
                    file.write(&buf[..len])?
                }
            };
            self.seek(SeekFrom::Current(n as i64)).unwrap();
            Ok(n)
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.archive.file.borrow_mut().flush()
    }
}
