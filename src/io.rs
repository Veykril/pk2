#![allow(clippy::option_map_unit_fn)]

//! General io for reading/writing from/to buffers.

use std::io::{self, SeekFrom};

use crate::constants::{
    PK2_CURRENT_DIR_IDENT, PK2_FILE_BLOCK_SIZE, PK2_FILE_ENTRY_SIZE, PK2_PARENT_DIR_IDENT,
};
use crate::error::OpenResult;
use crate::raw::block_chain::{PackBlock, PackBlockChain};
use crate::raw::entry::PackEntry;
use crate::raw::{BlockOffset, ChainIndex, EntryOffset, StreamOffset};
use crate::Blowfish;

/// Read a block at a given offset.
pub fn read_block_at<F: io::Seek + io::Read>(
    bf: Option<&Blowfish>,
    mut stream: F,
    BlockOffset(offset): BlockOffset,
) -> OpenResult<PackBlock> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    stream.seek(SeekFrom::Start(offset))?;
    stream.read_exact(&mut buf)?;
    bf.map(|bf| bf.decrypt(&mut buf));
    PackBlock::from_reader(&buf[..]).map_err(Into::into)
}

pub fn read_exact_at<F: io::Seek + io::Read>(
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    buf: &mut [u8],
) -> io::Result<()> {
    stream.seek(SeekFrom::Start(offset))?;
    stream.read_exact(buf)
}

pub fn read_at<F: io::Seek + io::Read>(
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    buf: &mut [u8],
) -> io::Result<usize> {
    stream.seek(SeekFrom::Start(offset))?;
    stream.read(buf)
}

#[inline]
fn stream_len<F: io::Seek>(mut stream: F) -> io::Result<u64> {
    stream.seek(SeekFrom::End(0))
}

/// Write/Update a block at the given block offset in the file.
pub fn write_block<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut stream: F,
    BlockOffset(offset): BlockOffset,
    block: &PackBlock,
) -> io::Result<()> {
    let mut buf = [0; PK2_FILE_BLOCK_SIZE];
    block.to_writer(&mut buf[..])?;
    bf.map(|bf| bf.encrypt(&mut buf));
    stream.seek(SeekFrom::Start(offset))?;
    stream.write_all(&buf)?;
    Ok(())
}

/// Write/Update an entry at the given entry offset in the file.
pub fn write_entry_at<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut stream: F,
    EntryOffset(offset): EntryOffset,
    entry: &PackEntry,
) -> io::Result<()> {
    let mut buf = [0; PK2_FILE_ENTRY_SIZE];
    entry.to_writer(&mut buf[..])?;
    bf.map(|bf| bf.encrypt(&mut buf));
    stream.seek(SeekFrom::Start(offset))?;
    stream.write_all(&buf)?;
    Ok(())
}

/// Write/Update a chain's entry at the given chain offset and entry index in
/// the file.
#[inline]
pub fn write_chain_entry<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    stream: F,
    chain: &PackBlockChain,
    entry_index: usize,
) -> io::Result<()> {
    debug_assert!(chain.contains_entry_index(entry_index));
    write_entry_at(
        bf,
        stream,
        chain.stream_offset_for_entry(entry_index).unwrap(),
        &chain[entry_index],
    )
}

/// Write data to the end of the file returning the offset of the written
/// data in the file.
pub fn append_data<F: io::Seek + io::Write>(
    mut stream: F,
    data: &[u8],
) -> io::Result<StreamOffset> {
    let stream_end = stream_len(&mut stream)?;
    stream.write_all(data)?;
    Ok(StreamOffset(stream_end))
}

/// Write raw data at the given offset into the buffer.
pub fn write_data_at<F: io::Seek + io::Write>(
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    data: &[u8],
) -> io::Result<()> {
    stream.seek(SeekFrom::Start(offset))?;
    stream.write_all(data)
}

/// Create a new [`PackBlockChain`] at the end of the buffer and update the
/// corresponding entry in the chain.
pub fn allocate_new_block_chain<F: io::Seek + io::Write>(
    blowfish: Option<&Blowfish>,
    mut stream: F,
    current_chain: &mut PackBlockChain,
    dir_name: &str,
    chain_entry_idx: usize,
) -> io::Result<PackBlockChain> {
    debug_assert!(current_chain.contains_entry_index(chain_entry_idx));
    let new_chain_offset = stream_len(&mut stream).map(ChainIndex)?;

    let entry = &mut current_chain[chain_entry_idx];
    debug_assert!(entry.is_empty());
    *entry = PackEntry::new_directory(dir_name, new_chain_offset, entry.next_block());

    let mut block = PackBlock::default();
    block[0] = PackEntry::new_directory(PK2_CURRENT_DIR_IDENT, new_chain_offset, None);
    block[1] = PackEntry::new_directory(PK2_PARENT_DIR_IDENT, current_chain.chain_index(), None);
    write_block(blowfish, &mut stream, new_chain_offset.into(), &block)?;

    let offset = current_chain
        .stream_offset_for_entry(chain_entry_idx)
        .unwrap();

    write_entry_at(blowfish, stream, offset, &current_chain[chain_entry_idx])?;
    Ok(PackBlockChain::from_blocks(vec![(
        new_chain_offset.into(),
        block,
    )]))
}

/// Create a new empty [`PackBlock`] at the end of the buffer.
pub fn allocate_empty_block<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut stream: F,
) -> io::Result<(BlockOffset, PackBlock)> {
    let offset = stream_len(&mut stream).map(BlockOffset)?;
    let block = PackBlock::default();
    write_block(bf, stream, offset, &block).and(Ok((offset, block)))
}

pub trait RawIo: Sized {
    fn from_reader<R: io::Read>(r: R) -> io::Result<Self>;
    fn to_writer<W: io::Write>(&self, w: W) -> io::Result<()>;
}
