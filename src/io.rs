use std::io;

use crate::constants::{
    PK2_CURRENT_DIR_IDENT, PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE, PK2_PARENT_DIR_IDENT,
};
use crate::error::Pk2Result;
use crate::raw::block_chain::{PackBlock, PackBlockChain};
use crate::raw::entry::PackEntry;
use crate::raw::{BlockOffset, ChainIndex, EntryOffset};
use crate::Blowfish;

pub fn read_block_at<F: io::Seek + io::Read>(
    bf: Option<&Blowfish>,
    mut file: F,
    BlockOffset(offset): BlockOffset,
) -> Pk2Result<PackBlock> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    file.seek(io::SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    bf.map(|bf| bf.decrypt(&mut buf));
    PackBlock::from_reader(&buf[..])
}

pub fn file_len<F: io::Seek>(mut file: F) -> io::Result<u64> {
    file.seek(io::SeekFrom::End(0))
}

pub fn write_block<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut file: F,
    BlockOffset(offset): BlockOffset,
    block: &PackBlock,
) -> Pk2Result<()> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    block.to_writer(&mut buf[..])?;
    bf.map(|bf| bf.encrypt(&mut buf));
    file.seek(io::SeekFrom::Start(offset))?;
    file.write_all(&buf)?;
    Ok(())
}

pub fn write_entry_at<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut file: F,
    EntryOffset(offset): EntryOffset,
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

pub fn create_new_block_chain<F: io::Seek + io::Write>(
    blowfish: Option<&Blowfish>,
    mut file: F,
    current_chain: &mut PackBlockChain,
    dir_name: &str,
    chain_entry_idx: usize,
) -> Pk2Result<PackBlockChain> {
    let new_chain_offset = crate::io::file_len(&mut file).map(ChainIndex)?;
    let entry = &mut current_chain[chain_entry_idx];
    *entry = PackEntry::new_directory(dir_name, new_chain_offset, entry.next_block());
    let offset = current_chain
        .file_offset_for_entry(chain_entry_idx)
        .unwrap();
    let mut block = PackBlock::default();
    block[0] = PackEntry::new_directory(PK2_CURRENT_DIR_IDENT, new_chain_offset, None);
    block[1] = PackEntry::new_directory(PK2_PARENT_DIR_IDENT, current_chain.chain_index(), None);
    write_block(blowfish, &mut file, new_chain_offset.into(), &block)?;
    write_entry_at(blowfish, file, offset, &current_chain[chain_entry_idx])?;
    Ok(PackBlockChain::from_blocks(vec![(
        new_chain_offset.into(),
        block,
    )]))
}

pub trait RawIo: Sized {
    fn from_reader<R: io::Read>(r: R) -> Pk2Result<Self>;
    fn to_writer<W: io::Write>(&self, w: W) -> io::Result<()>;
}
