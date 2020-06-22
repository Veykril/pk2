pub mod block_chain;
pub mod block_manager;
pub mod entry;
pub mod header;

use std::ops;

/// Offset into the stream for a given chain. This is also used as an index into
/// the block manager, hence the name.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainIndex(pub u64);

impl From<ChainIndex> for BlockOffset {
    #[inline]
    fn from(idx: ChainIndex) -> BlockOffset {
        BlockOffset(idx.0)
    }
}

/// Offset into the stream for a given block.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockOffset(pub u64);

/// Offset into the stream for a given entry.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryOffset(pub u64);

/// Offset into the stream for generic data.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamOffset(pub u64);

impl ops::Add for StreamOffset {
    type Output = Self;
    fn add(self, StreamOffset(rhs): Self) -> Self::Output {
        StreamOffset(self.0 + rhs)
    }
}
