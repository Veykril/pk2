//! Functionality to deal with the raw data of a pk2 archive file.

pub mod block_chain;
pub mod chain_index;
pub mod entry;
pub mod header;

use core::num::NonZeroU64;
use core::ops;

/// Offset into the stream for a given chain. This is also used as an index into
/// the block manager, hence the name.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainOffset(pub NonZeroU64);

/// Offset into the stream for a given block.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockOffset(pub NonZeroU64);

/// Offset into the stream for generic data.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamOffset(pub NonZeroU64);

impl ops::Add for StreamOffset {
    type Output = Self;
    fn add(self, StreamOffset(rhs): Self) -> Self::Output {
        StreamOffset(self.0.checked_add(rhs.get()).unwrap())
    }
}
