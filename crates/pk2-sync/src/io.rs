//! General io for reading/writing from/to buffers.

use std::io::{self, SeekFrom};
use std::num::NonZeroU64;

use pk2::block_chain::{PackBlock, PackBlockChain};
use pk2::blowfish::Blowfish;
use pk2::entry::PackEntry;
use pk2::{BlockOffset, ChainOffset, StreamOffset};

pub fn read_exact_at<F: io::Seek + io::Read>(
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    buf: &mut [u8],
) -> io::Result<()> {
    stream.seek(SeekFrom::Start(offset.get()))?;
    stream.read_exact(buf)
}

pub fn read_at<F: io::Seek + io::Read>(
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    buf: &mut [u8],
) -> io::Result<usize> {
    stream.seek(SeekFrom::Start(offset.get()))?;
    stream.read(buf)
}

fn stream_len<F: io::Seek>(mut stream: F) -> io::Result<NonZeroU64> {
    stream.seek(SeekFrom::End(0)).and_then(|offset| {
        NonZeroU64::new(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "empty stream"))
    })
}

/// Write/Update a block at the given block offset in the file.
pub fn write_block<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut stream: F,
    BlockOffset(offset): BlockOffset,
    block: &PackBlock,
) -> io::Result<()> {
    let mut buf = [0; PackBlock::PK2_FILE_BLOCK_SIZE];
    block.write_to(&mut buf);
    if let Some(bf) = bf {
        bf.encrypt(&mut buf);
    }
    stream.seek(SeekFrom::Start(offset.get()))?;
    stream.write_all(&buf)?;
    Ok(())
}

/// Write/Update an entry at the given entry offset in the file.
pub fn write_entry_at<F: io::Seek + io::Write>(
    bf: Option<&Blowfish>,
    mut stream: F,
    StreamOffset(offset): StreamOffset,
    entry: &PackEntry,
) -> io::Result<()> {
    let mut buf = [0; PackEntry::PK2_FILE_ENTRY_SIZE];
    entry.write_to(&mut buf);
    if let Some(bf) = bf {
        bf.encrypt(&mut buf);
    }
    stream.seek(SeekFrom::Start(offset.get()))?;
    stream.write_all(&buf)?;
    Ok(())
}

/// Write/Update a chain's entry at the given chain offset and entry index in
/// the file.
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
    stream.seek(SeekFrom::Start(offset.get()))?;
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
    let new_chain_offset = stream_len(&mut stream).map(ChainOffset)?;

    let entry = &mut current_chain[chain_entry_idx];
    debug_assert!(entry.is_empty());
    *entry = PackEntry::new_directory(dir_name, new_chain_offset, entry.next_block());

    let mut block = PackBlock::default();
    block[0] = PackEntry::new_directory(".", new_chain_offset, None);
    block[1] = PackEntry::new_directory("..", current_chain.chain_index(), None);
    write_block(blowfish, &mut stream, BlockOffset(new_chain_offset.0), &block)?;

    let offset = current_chain.stream_offset_for_entry(chain_entry_idx).unwrap();

    write_entry_at(blowfish, stream, offset, &current_chain[chain_entry_idx])?;
    Ok(PackBlockChain::from_blocks(vec![(BlockOffset(new_chain_offset.0), block)]))
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
