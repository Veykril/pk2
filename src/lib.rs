#![warn(clippy::all)]
#![allow(clippy::match_bool, clippy::vec_box)]
pub mod fs;

mod archive;
pub use self::archive::Pk2;

mod phys_file;
pub(crate) use self::phys_file::PhysicalFile;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ChainIndex(pub u64);
pub(crate) type Blowfish =
    block_modes::Ecb<blowfish::BlowfishLE, block_modes::block_padding::ZeroPadding>;

#[allow(non_snake_case)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FILETIME {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

/// Magic Numbers and definitions
#[allow(dead_code)]
pub(crate) mod constants {
    use super::ChainIndex;
    use super::FILETIME;
    use std::mem;

    pub const PK2_VERSION: u32 = 0x0100_0002;
    pub const PK2_SIGNATURE: &[u8; 30] =
        b"JoyMax File Manager!\x0a\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    pub const PK2_SALT: [u8; 10] = [0x03, 0xF8, 0xE4, 0x44, 0x88, 0x99, 0x3F, 0x64, 0xFE, 0x35];
    pub const PK2_CHECKSUM_STORED: usize = 3;
    pub const PK2_CHECKSUM: &[u8; 16] = b"Joymax Pak File\0";

    pub const PK2_FILE_ENTRY_SIZE: usize = mem::size_of::<RawPackFileEntry>();
    pub const PK2_FILE_BLOCK_ENTRY_COUNT: usize = 20;
    pub const PK2_FILE_BLOCK_SIZE: usize =
        mem::size_of::<[RawPackFileEntry; PK2_FILE_BLOCK_ENTRY_COUNT]>();

    pub(crate) const PK2_ROOT_BLOCK: ChainIndex =
        ChainIndex(mem::size_of::<RawPackHeader>() as u64);

    #[repr(packed)]
    pub struct RawPackHeader {
        signature: [u8; 30],
        version: u32,
        encrypted: u8,
        verify: [u8; 16],
        reserved: [u8; 205],
    }

    #[repr(packed)]
    pub struct RawPackFileEntry {
        ty: u8, //0 = Empty, 1 = Directory, 2  = File
        name: [u8; 81],
        access: FILETIME,
        create: FILETIME,
        modify: FILETIME,
        position: u64, // Position of data for files, position of children for directorys
        size: u32,
        next_block: u64,
        _padding: [u8; 2],
    }
}
