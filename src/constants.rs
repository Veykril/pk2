//! Magic Numbers and definitions
use std::mem;

use crate::data::ChainIndex;
use crate::filetime::FILETIME;

pub const PK2_VERSION: u32 = 0x0100_0002;
pub const PK2_SIGNATURE: &[u8; 30] = b"JoyMax File Manager!\n\0\0\0\0\0\0\0\0\0";
pub const PK2_SALT: &[u8; 10] = &[0x03, 0xF8, 0xE4, 0x44, 0x88, 0x99, 0x3F, 0x64, 0xFE, 0x35];
/// The number of bytes in the checksum that are actually stored in the header. Yes, the archive
/// only stores 3 bytes of the checksum...
pub const PK2_CHECKSUM_STORED: usize = 3;
/// The checksum value.
pub const PK2_CHECKSUM: &[u8; 16] = b"Joymax Pak File\0";

pub const PK2_FILE_ENTRY_SIZE: usize = mem::size_of::<RawPackFileEntry>();
pub const PK2_FILE_BLOCK_ENTRY_COUNT: usize = 20;
pub const PK2_FILE_BLOCK_SIZE: usize =
    mem::size_of::<[RawPackFileEntry; PK2_FILE_BLOCK_ENTRY_COUNT]>();

pub const PK2_ROOT_BLOCK: ChainIndex = ChainIndex(mem::size_of::<RawPackHeader>() as u64);
// Sentinel entry to give the root block a proper path descriptor
pub const PK2_ROOT_BLOCK_VIRTUAL: ChainIndex = ChainIndex(0);

pub const PK2_CURRENT_DIR_IDENT: &str = ".";
pub const PK2_PARENT_DIR_IDENT: &str = "..";

/// The in-file header layout.
#[allow(dead_code)]
#[repr(C, packed)]
pub struct RawPackHeader {
    pub signature: [u8; 30],
    pub version: u32,
    pub encrypted: u8,
    pub verify: [u8; 16],
    pub reserved: [u8; 205],
}

/// The structure of a single entry in a pack file.
#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct RawPackFileEntry {
    pub ty: u8, //0 = Empty, 1 = Directory, 2  = File
    pub name: [u8; 81],
    pub access: FILETIME,
    pub create: FILETIME,
    pub modify: FILETIME,
    pub position: u64, // Position of data for files, position of children for directorys
    pub size: u32,
    pub next_block: u64,
    pub _padding: [u8; 2],
}

impl RawPackFileEntry {
    pub const TY_EMPTY: u8 = 0;
    pub const TY_DIRECTORY: u8 = 1;
    pub const TY_FILE: u8 = 2;
}

#[cfg(test)]
unsafe impl bytemuck::Zeroable for RawPackFileEntry {}
#[cfg(test)]
unsafe impl bytemuck::Pod for RawPackFileEntry {}
