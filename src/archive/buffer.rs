use std::cell::RefCell;
use std::io::{self, Seek, SeekFrom, Write};

use crate::archive::{PackBlock, PackEntry};
use crate::constants::{PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE};
use crate::error::Pk2Result;
use crate::Blowfish;

pub(crate) struct ArchiveBuffer<B> {
    file: RefCell<B>,
    // UnsafeCell is being used here due to the blowfish lib requiring mutability, we don't lend
    // out borrows for more than a function call though so this is fine
    bf: Option<Blowfish>,
}

impl<B> ArchiveBuffer<B> {
    pub(crate) fn new(file: B, bf: Option<Blowfish>) -> Self {
        ArchiveBuffer {
            file: RefCell::new(file),
            bf,
        }
    }

    pub(crate) fn file(&self) -> std::cell::RefMut<'_, B> {
        self.file.borrow_mut()
    }

    pub(crate) fn bf(&self) -> Option<&Blowfish> {
        self.bf.as_ref()
    }

    #[inline]
    fn encrypt(&self, buf: &mut [u8]) {
        let _ = self.bf.as_ref().map(|bf| bf.encrypt(buf));
    }
}

impl<B: Seek> ArchiveBuffer<B> {
    pub(crate) fn len(&self) -> Pk2Result<u64> {
        self.file
            .borrow_mut()
            .seek(SeekFrom::End(0))
            .map_err(Into::into)
    }
}

impl<B> ArchiveBuffer<B>
where
    B: Write + Seek,
{
    pub(crate) fn write_entry_at(&self, offset: u64, entry: &PackEntry) -> io::Result<()> {
        let mut buf = [0; PK2_FILE_ENTRY_SIZE];
        entry.to_writer(&mut buf[..])?;
        self.encrypt(&mut buf);
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buf)?;
        Ok(())
    }

    /// Write data to the end of the file returning the offset of the written
    /// data in the file.
    pub(crate) fn write_new_data_buffer(&self, data: &[u8]) -> io::Result<u64> {
        let mut file = self.file.borrow_mut();
        let file_end = file.seek(SeekFrom::End(0))?;
        file.write_all(data)?;
        Ok(file_end)
    }

    pub(crate) fn write_data_buffer_at(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)
    }

    pub(crate) fn write_block(&self, block: &PackBlock) -> Pk2Result<()> {
        let mut buf = [0; PK2_FILE_BLOCK_SIZE];
        block.to_writer(&mut buf[..])?;
        self.encrypt(&mut buf);
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(block.offset()))?;
        file.write_all(&buf)?;
        Ok(())
    }
}

impl<B: Write> Write for ArchiveBuffer<B> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.borrow_mut().write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.file.borrow_mut().flush()
    }
}
