use std::io::{self, Result, Seek, SeekFrom, Write};

use crate::archive::{PackEntry, Pk2};

pub struct FileMut<'a> {
    archive: &'a mut Pk2,
    chain: u64,
    entry_index: usize,
    seek_pos: usize,
    data: Vec<u8>,
}

impl<'a> FileMut<'a> {
    pub(in crate) fn new(
        archive: &'a mut Pk2,
        chain: u64,
        entry_index: usize,
        data: Vec<u8>,
    ) -> Self {
        FileMut {
            archive,
            chain,
            entry_index,
            seek_pos: 0,
            data,
        }
    }

    #[inline]
    fn entry(&self) -> &PackEntry {
        &self.archive.block_mgr.chains[&self.chain][self.entry_index]
    }

    #[inline]
    fn entry_mut(&mut self) -> &mut PackEntry {
        &mut self.archive.block_mgr.chains.get_mut(&self.chain).unwrap()[self.entry_index]
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
}

impl Seek for FileMut<'_> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let size = self.data.len();
        let (base_pos, offset) = match pos {
            SeekFrom::Start(n) => {
                self.seek_pos = (n as usize).min(size);
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
            self.seek_pos = new_pos as usize;
            Ok(self.seek_pos as u64)
        }
    }
}

impl Write for FileMut<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let new_pos = (self.seek_pos + buf.len()).max(!0u32 as usize);
        let data_len = self.data.len();
        if new_pos >= data_len {
            self.data[self.seek_pos..data_len].copy_from_slice(&buf[..data_len - self.seek_pos]);
            self.data
                .extend_from_slice(&buf[data_len - self.seek_pos..]);
        } else {
            self.data[self.seek_pos..new_pos].copy_from_slice(buf);
        }
        self.seek_pos = new_pos;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let (file_data_pos, file_data_size) = match *self.entry() {
            PackEntry::File { pos_data, size, .. } => (pos_data, size),
            _ => unreachable!(),
        };
        // new unwritten file
        if file_data_pos == 0 {
            // allocate new data buffer in file
            let data_pos = self.archive.file.write_new_data_buffer(&self.data)?;
            let data_len = self.data.len();
            match self.entry_mut() {
                PackEntry::File { pos_data, size, .. } => {
                    *pos_data = data_pos;
                    *size = data_len as u32;
                }
                _ => unreachable!(),
            };
            let entry_offset = self.archive.block_mgr.chains[&self.chain]
                .get_file_offset_for_entry(self.entry_index)
                .unwrap();
            self.archive.file.write_entry_at(
                entry_offset,
                &self.archive.block_mgr.chains[&self.chain][self.entry_index],
            )?;
        } else if file_data_size >= self.data.len() as u32 {
            self.archive
                .file
                .write_data_buffer_at(file_data_pos, &self.data)?;
        } else {
            unimplemented!("relocate data block")
        }
        Ok(())
    }
}

impl Drop for FileMut<'_> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
