use block_modes::BlockMode;

use std::cell::{RefCell, UnsafeCell};
use std::fs::File;
use std::io::{Read, Result, Seek, SeekFrom, Write};

use crate::archive::{PackBlock, PackEntry};
use crate::constants::{PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE};
use crate::Blowfish;

pub struct PhysicalFile {
    file: RefCell<File>,
    bf: Option<UnsafeCell<Blowfish>>,
}

impl Drop for PhysicalFile {
    fn drop(&mut self) {
        let len = self.len().unwrap() as usize;
        // Apparently 4kb is the minimum archive size that the dll requires
        if len < 4096 {
            let _ = self.file.borrow_mut().write_all(&[0; 4096][..4096 - len]);
        }
    }
}

impl PhysicalFile {
    pub fn new(file: File, bf: Option<Blowfish>) -> Self {
        PhysicalFile {
            file: RefCell::new(file),
            bf: bf.map(UnsafeCell::new),
        }
    }

    pub fn len(&self) -> Result<u64> {
        self.file.borrow_mut().seek(SeekFrom::End(0))
    }

    #[inline]
    fn encrypt(&self, buf: &mut [u8]) {
        let _ = self
            .bf
            .as_ref()
            .map(|bf| unsafe { &mut *bf.get() }.encrypt_nopad(buf));
    }

    #[inline]
    fn decrypt(&self, buf: &mut [u8]) {
        let _ = self
            .bf
            .as_ref()
            .map(|bf| unsafe { &mut *bf.get() }.decrypt_nopad(buf));
    }

    pub fn write_entry_at(&self, offset: u64, entry: &PackEntry) -> Result<()> {
        let mut buf = [0; PK2_FILE_ENTRY_SIZE];
        entry.to_writer(&mut buf[..])?;
        self.encrypt(&mut buf);
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buf)
    }

    /// Write data to the end of the file returning the offset of the written
    /// data in the file.
    pub fn write_new_data_buffer(&self, data: &[u8]) -> Result<u64> {
        let mut file = self.file.borrow_mut();
        let file_end = file.seek(SeekFrom::End(0))?;
        file.write_all(data)?;
        Ok(file_end)
    }

    pub fn write_data_buffer_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)
    }

    pub fn write_block(&self, block: &PackBlock) -> Result<()> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        block.to_writer(&mut buf[..])?;
        self.encrypt(&mut buf);
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(block.offset))?;
        file.write_all(&buf)?;
        Ok(())
    }

    pub fn read_block_at(&self, offset: u64) -> Result<PackBlock> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buf)?;
        self.decrypt(&mut buf);
        PackBlock::from_reader(&buf[..], offset)
    }

    pub fn file(&self) -> std::cell::RefMut<'_, File> {
        self.file.borrow_mut()
    }
}

impl Write for PhysicalFile {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.file.borrow_mut().write(buf)
    }

    #[inline]
    fn flush(&mut self) -> Result<()> {
        self.file.borrow_mut().flush()
    }
}
