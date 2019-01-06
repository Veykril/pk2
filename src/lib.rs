#![allow(clippy::cast_lossless)]
#[macro_use]
extern crate derivative;

pub mod archive;
pub mod fs;

pub(in crate) type Blowfish =
    block_modes::Ecb<blowfish::BlowfishLE, block_modes::block_padding::ZeroPadding>;

#[allow(non_snake_case)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FILETIME {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

pub(in crate) mod constants {
    use crate::FILETIME;

    pub const PK2_VERSION: u32 = 0x0100_0002;
    pub const PK2_SIGNATURE: &[u8; 30] =
        b"JoyMax File Manager!\x0a\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    pub const PK2_SALT: [u8; 10] = [0x03, 0xF8, 0xE4, 0x44, 0x88, 0x99, 0x3F, 0x64, 0xFE, 0x35];
    pub const PK2_CHECKSUM_STORED: usize = 3;
    pub const PK2_CHECKSUM: &[u8; 16] = b"Joymax Pak File\0";

    pub const PK2_FILE_ENTRY_SIZE: usize = std::mem::size_of::<RawPackFileEntry>();
    pub const PK2_FILE_BLOCK_ENTRY_COUNT: usize = 20;
    pub const PK2_FILE_BLOCK_SIZE: usize =
        std::mem::size_of::<[RawPackFileEntry; PK2_FILE_BLOCK_ENTRY_COUNT]>();

    pub const PK2_ROOT_BLOCK: u64 = std::mem::size_of::<RawPackHeader>() as u64;

    #[repr(packed)]
    pub struct RawPackHeader {
        pub signature: [u8; 30],
        pub version: u32,
        pub encrypted: u8,
        pub verify: [u8; 16],
        pub reserved: [u8; 205],
    }

    #[repr(packed)]
    pub struct RawPackFileEntry {
        pub ty: u8, //0 = Empty, 1 = Directory, 2  = File
        pub name: [u8; 81],
        pub access: FILETIME,
        pub create: FILETIME,
        pub modify: FILETIME,
        pub position: u64, // Position of data for files, position of children for directorys
        pub size: u32,
        pub next_chain: u64,
        pub _padding: [u8; 2],
    }
}
