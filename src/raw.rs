pub mod block_chain;
pub mod block_manager;
pub mod entry;
pub mod header;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainIndex(pub u64);
