use block_modes::BlockMode;

use std::cell::{RefCell, RefMut};
use std::fs::File;
use std::io::{Read, Result, Seek, SeekFrom, Write};

use crate::archive::{PackBlock, PackEntry};
use crate::constants::{PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE};
use crate::Blowfish;

pub(in crate) struct PhysFile {
    file: RefCell<File>,
    bf: Blowfish,
}

impl PhysFile {
    pub fn new(file: File, bf: Blowfish) -> Self {
        PhysFile {
            file: RefCell::new(file),
            bf,
        }
    }

    pub fn len(&self) -> Result<u64> {
        self.file.borrow().metadata().map(|m| m.len())
    }

    pub(in crate) fn write_entry_at(&mut self, offset: u64, entry: &PackEntry) -> Result<()> {
        let mut buf = [0; PK2_FILE_ENTRY_SIZE];
        entry.to_writer(&mut buf[..])?;
        let _ = self.bf.encrypt_nopad(&mut buf);
        self.borrow_mut().seek(SeekFrom::Start(offset))?;
        self.borrow_mut().write_all(&buf)
    }

    pub(in crate) fn write_new_data_buffer(&mut self, data: &[u8]) -> Result<u64> {
        let file_end = self.borrow_mut().seek(SeekFrom::End(0))?;
        self.borrow_mut().write_all(data)?;
        Ok(file_end)
    }

    pub(in crate) fn write_data_buffer_at(&mut self, offset: u64, data: &[u8]) -> Result<u64> {
        let file_end = self.borrow_mut().seek(SeekFrom::Start(offset))?;
        self.borrow_mut().write_all(data)?;
        Ok(file_end)
    }

    pub(in crate) fn create_new_block_at(&mut self, offset: u64) -> Result<PackBlock> {
        let mut block = PackBlock::default();
        block.offset = offset;
        self.write_block(&block)?;
        Ok(block)
    }

    pub(in crate) fn write_block(&mut self, block: &PackBlock) -> Result<()> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        block.to_writer(&mut buf[..])?;
        let _ = self.bf.encrypt_nopad(&mut buf);
        self.borrow_mut().seek(SeekFrom::Start(block.offset))?;
        self.borrow_mut().write_all(&buf)?;
        Ok(())
    }

    pub(in crate) fn read_block_at(&mut self, offset: u64) -> Result<PackBlock> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        self.borrow_mut().seek(SeekFrom::Start(offset))?;
        self.borrow_mut().read_exact(&mut buf)?;
        let _ = self.bf.decrypt_nopad(&mut buf);
        PackBlock::from_reader(&buf[..], offset)
    }

    #[inline]
    pub fn borrow_mut(&self) -> RefMut<File> {
        self.file.borrow_mut()
    }
}
