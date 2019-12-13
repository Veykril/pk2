use std::io;

use crate::constants::{PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE};
use crate::error::Pk2Result;
use crate::raw::block_chain::PackBlock;
use crate::raw::entry::PackEntry;
use crate::Blowfish;

pub fn read_block_at<F: io::Seek + io::Read>(
    bf: Option<&Blowfish>,
    mut file: F,
    offset: u64,
) -> Pk2Result<PackBlock> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    file.seek(io::SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    bf.map(|bf| bf.decrypt(&mut buf));
    PackBlock::from_reader(&buf[..], offset)
}

pub fn file_len<F: io::Seek>(mut file: F) -> io::Result<u64> {
    file.seek(io::SeekFrom::End(0))
}

pub fn write_block<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut file: F,
    block: &PackBlock,
) -> Pk2Result<()> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    block.to_writer(&mut buf[..])?;
    bf.map(|bf| bf.encrypt(&mut buf));
    file.seek(io::SeekFrom::Start(block.offset()))?;
    file.write_all(&buf)?;
    Ok(())
}

pub fn write_entry_at<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut file: F,
    offset: u64,
    entry: &PackEntry,
) -> io::Result<()> {
    let mut buf = [0; PK2_FILE_ENTRY_SIZE];
    entry.to_writer(&mut buf[..])?;
    bf.map(|bf| bf.encrypt(&mut buf));
    file.seek(io::SeekFrom::Start(offset))?;
    file.write_all(&buf)?;
    Ok(())
}

/// Write data to the end of the file returning the offset of the written
/// data in the file.
pub fn write_new_data_buffer<F: io::Seek + io::Write>(mut file: F, data: &[u8]) -> io::Result<u64> {
    let file_end = file.seek(io::SeekFrom::End(0))?;
    file.write_all(data)?;
    Ok(file_end)
}

pub fn write_data_buffer_at<F: io::Seek + io::Write>(
    mut file: F,
    offset: u64,
    data: &[u8],
) -> io::Result<()> {
    file.seek(io::SeekFrom::Start(offset))?;
    file.write_all(data)
}
