pub mod block_chain;
pub mod block_manager;
pub mod entry;
pub mod header;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainIndex(pub u64);

impl From<ChainIndex> for BlockOffset {
    #[inline]
    fn from(idx: ChainIndex) -> BlockOffset {
        BlockOffset(idx.0)
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockOffset(pub u64);

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryOffset(pub u64);
