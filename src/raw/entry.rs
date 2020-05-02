use byteorder::{ReadBytesExt, WriteBytesExt, LE};

use std::io::{Read, Result as IoResult, Write};
use std::num::NonZeroU64;
use std::time::SystemTime;

use super::{BlockOffset, ChainIndex};
use crate::constants::{PK2_CURRENT_DIR_IDENT, PK2_FILE_ENTRY_SIZE, PK2_PARENT_DIR_IDENT};
use crate::error::{Error, Pk2Result};
use crate::io::RawIo;
use crate::FILETIME;

#[derive(Clone, Eq, PartialEq)]
pub struct EmptyEntry {
    next_block: Option<NonZeroU64>,
}

impl EmptyEntry {
    #[inline]
    fn new(next_block: Option<NonZeroU64>) -> Self {
        EmptyEntry { next_block }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct DirectoryEntry {
    name: String,
    pub(crate) access_time: FILETIME,
    pub(crate) create_time: FILETIME,
    pub(crate) modify_time: FILETIME,
    pos_children: ChainIndex,
    next_block: Option<NonZeroU64>,
}

impl DirectoryEntry {
    fn new(name: String, pos_children: ChainIndex, next_block: Option<NonZeroU64>) -> Self {
        let ftime = FILETIME::now();
        DirectoryEntry {
            name,
            access_time: ftime,
            create_time: ftime,
            modify_time: ftime,
            pos_children,
            next_block,
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn access_time(&self) -> Option<SystemTime> {
        self.access_time.into_systime()
    }

    pub fn create_time(&self) -> Option<SystemTime> {
        self.create_time.into_systime()
    }

    pub fn modify_time(&self) -> Option<SystemTime> {
        self.modify_time.into_systime()
    }

    #[inline]
    pub fn children_position(&self) -> ChainIndex {
        self.pos_children
    }

    #[inline]
    pub fn next_block(&self) -> Option<NonZeroU64> {
        self.next_block
    }

    #[inline]
    pub fn is_current_link(&self) -> bool {
        self.name == PK2_CURRENT_DIR_IDENT
    }

    #[inline]
    pub fn is_parent_link(&self) -> bool {
        self.name == PK2_PARENT_DIR_IDENT
    }

    #[inline]
    pub fn is_normal_link(&self) -> bool {
        !(self.is_current_link() || self.is_parent_link())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct FileEntry {
    name: String,
    pub(crate) access_time: FILETIME,
    pub(crate) create_time: FILETIME,
    pub(crate) modify_time: FILETIME,
    pub(crate) pos_data: u64,
    pub(crate) size: u32,
    next_block: Option<NonZeroU64>,
}

impl FileEntry {
    pub(crate) fn new(
        name: String,
        pos_data: u64,
        size: u32,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        let ftime = FILETIME::now();
        FileEntry {
            name,
            access_time: ftime,
            create_time: ftime,
            modify_time: ftime,
            pos_data,
            size,
            next_block,
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn access_time(&self) -> Option<SystemTime> {
        self.access_time.into_systime()
    }

    pub fn create_time(&self) -> Option<SystemTime> {
        self.create_time.into_systime()
    }

    pub fn modify_time(&self) -> Option<SystemTime> {
        self.modify_time.into_systime()
    }

    #[inline]
    pub fn pos_data(&self) -> u64 {
        self.pos_data
    }

    #[inline]
    pub fn size(&self) -> u32 {
        self.size
    }

    #[inline]
    pub fn next_block(&self) -> Option<NonZeroU64> {
        self.next_block
    }
}

/// An entry of a [`PackBlock`].
#[derive(Clone, Eq, PartialEq)]
pub enum PackEntry {
    Empty(EmptyEntry),
    Directory(DirectoryEntry),
    File(FileEntry),
}

impl Default for PackEntry {
    fn default() -> Self {
        PackEntry::Empty(EmptyEntry::new(None))
    }
}

impl PackEntry {
    pub fn new_directory(
        name: impl Into<String>,
        pos_children: ChainIndex,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        PackEntry::Directory(DirectoryEntry::new(name.into(), pos_children, next_block))
    }

    pub fn new_file(
        name: impl Into<String>,
        pos_data: u64,
        size: u32,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        PackEntry::File(FileEntry::new(name.into(), pos_data, size, next_block))
    }

    pub fn new_empty(next_block: Option<NonZeroU64>) -> Self {
        PackEntry::Empty(EmptyEntry::new(next_block))
    }

    #[inline]
    pub fn as_directory(&self) -> Option<&DirectoryEntry> {
        match self {
            PackEntry::Directory(entry) => Some(entry),
            _ => None,
        }
    }

    #[inline]
    pub fn as_directory_mut(&mut self) -> Option<&mut DirectoryEntry> {
        match self {
            PackEntry::Directory(entry) => Some(entry),
            _ => None,
        }
    }

    #[inline]
    pub fn as_file(&self) -> Option<&FileEntry> {
        match self {
            PackEntry::File(entry) => Some(entry),
            _ => None,
        }
    }

    #[inline]
    pub fn as_file_mut(&mut self) -> Option<&mut FileEntry> {
        match self {
            PackEntry::File(entry) => Some(entry),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        let next_block = match *self {
            PackEntry::Empty(EmptyEntry { next_block })
            | PackEntry::Directory(DirectoryEntry { next_block, .. })
            | PackEntry::File(FileEntry { next_block, .. }) => next_block,
        };
        *self = PackEntry::new_empty(next_block);
    }

    pub fn next_block(&self) -> Option<NonZeroU64> {
        match *self {
            PackEntry::Empty(EmptyEntry { next_block })
            | PackEntry::Directory(DirectoryEntry { next_block, .. })
            | PackEntry::File(FileEntry { next_block, .. }) => next_block,
        }
    }

    pub fn set_next_block(&mut self, BlockOffset(nc): BlockOffset) {
        match self {
            PackEntry::Empty(EmptyEntry { next_block, .. })
            | PackEntry::Directory(DirectoryEntry { next_block, .. })
            | PackEntry::File(FileEntry { next_block, .. }) => *next_block = NonZeroU64::new(nc),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            PackEntry::Empty(_) => None,
            PackEntry::Directory(DirectoryEntry { name, .. })
            | PackEntry::File(FileEntry { name, .. }) => Some(name),
        }
    }

    pub fn name_eq_ignore_ascii_case(&self, other: &str) -> bool {
        self.name()
            .map(|this| this.eq_ignore_ascii_case(other))
            .unwrap_or(false)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            PackEntry::Empty(_) => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_file(&self) -> bool {
        match self {
            PackEntry::File(_) => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_dir(&self) -> bool {
        match self {
            PackEntry::Directory(_) => true,
            _ => false,
        }
    }
}

use std::mem;
impl RawIo for PackEntry {
    /// Reads an entry from the given Read instance always reading exactly
    /// PK2_FILE_ENTRY_SIZE bytes.
    fn from_reader<R: Read>(mut r: R) -> Pk2Result<Self> {
        match r.read_u8()? {
            0 => {
                r.read_exact(
                    &mut [0; PK2_FILE_ENTRY_SIZE
                        - mem::size_of::<u64>()
                        - mem::size_of::<u16>()
                        - mem::size_of::<u8>()],
                )?;
                let next_block = NonZeroU64::new(r.read_u64::<LE>()?);
                r.read_u16::<LE>()?;
                Ok(PackEntry::new_empty(next_block))
            }
            ty @ 1 | ty @ 2 => {
                let name = {
                    let mut buf = [0; 81];
                    r.read_exact(&mut buf)?;
                    let end = buf
                        .iter()
                        .position(|b| *b == 0)
                        .unwrap_or_else(|| buf.len());
                    #[cfg(feature = "euc")]
                    let name = encoding_rs::EUC_KR
                        .decode_without_bom_handling(&buf[..end])
                        .0;
                    #[cfg(not(feature = "euc"))]
                    let name = String::from_utf8_lossy(&buf[..end]);
                    name.into_owned()
                };
                let access_time = FILETIME {
                    dwLowDateTime: r.read_u32::<LE>()?,
                    dwHighDateTime: r.read_u32::<LE>()?,
                };
                let create_time = FILETIME {
                    dwLowDateTime: r.read_u32::<LE>()?,
                    dwHighDateTime: r.read_u32::<LE>()?,
                };
                let modify_time = FILETIME {
                    dwLowDateTime: r.read_u32::<LE>()?,
                    dwHighDateTime: r.read_u32::<LE>()?,
                };
                let position = r.read_u64::<LE>()?;
                let size = r.read_u32::<LE>()?;
                let next_block = NonZeroU64::new(r.read_u64::<LE>()?);
                r.read_u16::<LE>()?; //padding

                Ok(if ty == 1 {
                    PackEntry::Directory(DirectoryEntry {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        pos_children: ChainIndex(position),
                        next_block,
                    })
                } else {
                    PackEntry::File(FileEntry {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        pos_data: position,
                        size,
                        next_block,
                    })
                })
            }
            _ => Err(Error::CorruptedFile),
        }
    }

    fn to_writer<W: Write>(&self, mut w: W) -> IoResult<()> {
        match self {
            PackEntry::Empty(EmptyEntry { next_block }) => {
                w.write_all(
                    &[0; PK2_FILE_ENTRY_SIZE - mem::size_of::<u64>() - mem::size_of::<u16>()],
                )?;
                w.write_u64::<LE>(next_block.map_or(0, NonZeroU64::get))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
            PackEntry::Directory(DirectoryEntry {
                name,
                access_time,
                create_time,
                modify_time,
                pos_children: ChainIndex(position),
                next_block,
            })
            | PackEntry::File(FileEntry {
                name,
                access_time,
                create_time,
                modify_time,
                pos_data: position,
                next_block,
                ..
            }) => {
                w.write_u8(if self.is_dir() { 1 } else { 2 })?;
                #[cfg(feature = "euc")]
                let mut encoded = encoding_rs::EUC_KR.encode(name).0.into_owned();
                #[cfg(not(feature = "euc"))]
                let mut encoded = name.as_bytes().to_owned();
                encoded.resize(81, 0);
                w.write_all(&encoded)?;
                w.write_u32::<LE>(access_time.dwLowDateTime)?;
                w.write_u32::<LE>(access_time.dwHighDateTime)?;
                w.write_u32::<LE>(create_time.dwLowDateTime)?;
                w.write_u32::<LE>(create_time.dwHighDateTime)?;
                w.write_u32::<LE>(modify_time.dwLowDateTime)?;
                w.write_u32::<LE>(modify_time.dwHighDateTime)?;
                w.write_u64::<LE>(*position)?;
                w.write_u32::<LE>(if let PackEntry::File(FileEntry { size, .. }) = self {
                    *size
                } else {
                    0
                })?;
                w.write_u64::<LE>(next_block.map_or(0, NonZeroU64::get))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
        }
    }
}
