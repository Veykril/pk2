#![allow(clippy::cast_lossless)]
mod archive;
pub mod fs;

pub use self::archive::Pk2;

pub(in crate) type Blowfish =
    block_modes::Ecb<blowfish::BlowfishLE, block_modes::block_padding::ZeroPadding>;

#[allow(non_snake_case)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FILETIME {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

pub(in crate) mod constants {
    use crate::FILETIME;

    pub(in crate) const PK2_VERSION: u32 = 0x0100_0002;
    pub(in crate) const PK2_SIGNATURE: &[u8; 30] =
        b"JoyMax File Manager!\x0a\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    pub(in crate) const PK2_SALT: [u8; 10] =
        [0x03, 0xF8, 0xE4, 0x44, 0x88, 0x99, 0x3F, 0x64, 0xFE, 0x35];
    pub(in crate) const PK2_CHECKSUM_STORED: usize = 3;
    pub(in crate) const PK2_CHECKSUM: &[u8; 16] = b"Joymax Pak File\0";

    pub(in crate) const PK2_FILE_ENTRY_SIZE: usize = std::mem::size_of::<RawPackFileEntry>();
    pub(in crate) const PK2_FILE_BLOCK_ENTRY_COUNT: usize = 20;
    pub(in crate) const PK2_FILE_BLOCK_SIZE: usize =
        std::mem::size_of::<[RawPackFileEntry; PK2_FILE_BLOCK_ENTRY_COUNT]>();

    pub(in crate) const PK2_ROOT_BLOCK: u64 = std::mem::size_of::<RawPackHeader>() as u64;

    #[allow(dead_code)]
    #[repr(packed)]
    pub(in crate) struct RawPackHeader {
        pub(in crate) signature: [u8; 30],
        pub(in crate) version: u32,
        pub(in crate) encrypted: u8,
        pub(in crate) verify: [u8; 16],
        pub(in crate) reserved: [u8; 205],
    }

    #[allow(dead_code)]
    #[repr(packed)]
    pub(in crate) struct RawPackFileEntry {
        pub(in crate) ty: u8, //0 = Empty, 1 = Directory, 2  = File
        pub(in crate) name: [u8; 81],
        pub(in crate) access: FILETIME,
        pub(in crate) create: FILETIME,
        pub(in crate) modify: FILETIME,
        pub(in crate) position: u64, // Position of data for files, position of children for directorys
        pub(in crate) size: u32,
        pub(in crate) next_chain: u64,
        pub(in crate) _padding: [u8; 2],
    }
}
