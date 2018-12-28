use std::io::{self, Read, Result, Seek, SeekFrom, Write};
use std::path::Path;

use crate::archive::{Archive, PackEntry};
use crate::PackIndex;
use core::ops;

#[derive(Derivative)]
#[derivative(Debug)]
#[derive(Copy, Clone)]
pub struct File<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a Archive,
    chain: PackIndex,
    pos: u64,
}

impl<'a> File<'a> {
    pub fn open<P: AsRef<Path>>(archive: &'a Archive, path: P) -> Result<Self> {
        archive.open_file(path)
    }

    pub(crate) fn new(archive: &'a Archive, chain: PackIndex) -> Self {
        File {
            archive,
            chain,
            pos: 0,
        }
    }

    #[inline]
    fn entry(&self) -> &PackEntry {
        &self.archive.blockchains[&self.chain.0][self.chain.1]
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.entry() {
            PackEntry::File { name, .. } => name,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn pos_data_and_size(&self) -> (u64, u32) {
        match *self.entry() {
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct FileMut<'a> {
    #[derivative(Debug = "ignore")]
    archive: &'a mut Archive,
    chain: PackIndex,
    pos: u64,
}

impl<'a> FileMut<'a> {
    pub fn open<P: AsRef<Path>>(archive: &'a mut Archive, path: P) -> Result<Self> {
        archive.open_file_mut(path)
    }

    pub(crate) fn new(archive: &'a mut Archive, chain: PackIndex) -> Self {
        FileMut {
            archive,
            chain,
            pos: 0,
        }
    }

    #[inline]
    fn entry_mut(&mut self) -> &mut PackEntry {
        &mut self.archive.blockchains.get_mut(&self.chain.0).unwrap()[self.chain.1]
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

impl<'a> ops::Deref for FileMut<'a> {
    type Target = File<'a>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self as *const _ as *const File) }
    }
}

impl<'a> ops::DerefMut for FileMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self as *mut _ as *mut File) }
    }
}
