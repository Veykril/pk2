use std::io::{self, Read, Result, Seek, SeekFrom};
use std::path::Path;

use crate::archive::{PackEntry, Pk2};

#[derive(Copy, Clone)]
pub struct File<'a> {
    archive: &'a Pk2,
    entry: &'a PackEntry,
    seek_pos: u64,
}

impl<'a> File<'a> {
    pub fn open<P: AsRef<Path>>(archive: &'a Pk2, path: P) -> Result<Self> {
        archive.open_file(path)
    }

    pub(in crate) fn new(archive: &'a Pk2, entry: &'a PackEntry) -> Self {
        File {
            archive,
            entry,
            seek_pos: 0,
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.entry {
            PackEntry::File { name, .. } => name,
            _ => unreachable!(),
        }
    }

    pub fn path(&self) -> &str {
        unimplemented!()
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
                    file.seek(SeekFrom::Start(pos_data + self.seek_pos))?;
                    let len = (size as usize - self.seek_pos as usize).min(buf.len());
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
                self.seek_pos = n.min(size);
                return Ok(n);
            }
            SeekFrom::End(n) => (size, n),
            SeekFrom::Current(n) => (self.seek_pos, n),
        };
        let new_pos = base_pos as i64 + offset;
        if new_pos < 0 || new_pos > size as i64 {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or size overflowing position",
            ))
        } else {
            self.seek_pos = new_pos as u64;
            Ok(self.seek_pos)
        }
    }
}
/*
impl Write for File<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            Ok(0)
        } else {
            let (pos_data, size) = self.pos_data_and_size();
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
*/
