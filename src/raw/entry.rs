use byteorder::{ReadBytesExt, WriteBytesExt, LE};

use std::io::{Read, Result as IoResult, Write};
use std::mem;
use std::num::NonZeroU64;
use std::time::SystemTime;

use crate::constants::{PK2_CURRENT_DIR_IDENT, PK2_FILE_ENTRY_SIZE, PK2_PARENT_DIR_IDENT};
use crate::filetime::FILETIME;
use crate::io::RawIo;
use crate::raw::{BlockOffset, ChainIndex, StreamOffset};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    name: Box<str>,
    pub(crate) access_time: FILETIME,
    pub(crate) create_time: FILETIME,
    pub(crate) modify_time: FILETIME,
    pos_children: ChainIndex,
}

impl DirectoryEntry {
    fn new(name: Box<str>, pos_children: ChainIndex) -> Self {
        let ftime = FILETIME::now();
        DirectoryEntry {
            name,
            access_time: ftime,
            create_time: ftime,
            modify_time: ftime,
            pos_children,
        }
    }

    #[cfg(test)]
    fn new_untimed(name: Box<str>, pos_children: ChainIndex) -> Self {
        DirectoryEntry {
            name,
            access_time: FILETIME::default(),
            create_time: FILETIME::default(),
            modify_time: FILETIME::default(),
            pos_children,
        }
    }

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

    pub fn children_position(&self) -> ChainIndex {
        self.pos_children
    }

    pub fn is_current_link(&self) -> bool {
        self.name() == PK2_CURRENT_DIR_IDENT
    }

    pub fn is_parent_link(&self) -> bool {
        self.name() == PK2_PARENT_DIR_IDENT
    }

    pub fn is_normal_link(&self) -> bool {
        !(self.is_current_link() || self.is_parent_link())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    name: Box<str>,
    pub(crate) access_time: FILETIME,
    pub(crate) create_time: FILETIME,
    pub(crate) modify_time: FILETIME,
    pub(crate) pos_data: StreamOffset,
    pub(crate) size: u32,
}

impl FileEntry {
    pub(crate) fn new(name: Box<str>, pos_data: StreamOffset, size: u32) -> Self {
        let ftime = FILETIME::now();
        FileEntry {
            name,
            access_time: ftime,
            create_time: ftime,
            modify_time: ftime,
            pos_data,
            size,
        }
    }

    #[cfg(test)]
    fn new_untimed(name: Box<str>, pos_data: StreamOffset, size: u32) -> Self {
        FileEntry {
            name,
            access_time: FILETIME::default(),
            create_time: FILETIME::default(),
            modify_time: FILETIME::default(),
            pos_data,
            size,
        }
    }

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

    pub fn pos_data(&self) -> StreamOffset {
        self.pos_data
    }

    pub fn size(&self) -> u32 {
        self.size
    }
}

/// An entry of a [`PackBlock`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackEntry {
    pub(crate) kind: Option<PackEntryKind>,
    next_block: Option<NonZeroU64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackEntryKind {
    Directory(DirectoryEntry),
    File(FileEntry),
}

impl PackEntry {
    pub fn new_directory(
        name: impl Into<Box<str>>,
        pos_children: ChainIndex,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        PackEntry {
            kind: Some(PackEntryKind::Directory(DirectoryEntry::new(name.into(), pos_children))),
            next_block,
        }
    }

    pub fn new_file(
        name: impl Into<Box<str>>,
        pos_data: StreamOffset,
        size: u32,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        PackEntry {
            kind: Some(PackEntryKind::File(FileEntry::new(name.into(), pos_data, size))),
            next_block,
        }
    }

    pub fn new_empty(next_block: Option<NonZeroU64>) -> Self {
        PackEntry { kind: None, next_block }
    }

    pub fn as_directory(&self) -> Option<&DirectoryEntry> {
        match &self.kind {
            Some(PackEntryKind::Directory(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_file(&self) -> Option<&FileEntry> {
        match &self.kind {
            Some(PackEntryKind::File(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_file_mut(&mut self) -> Option<&mut FileEntry> {
        match &mut self.kind {
            Some(PackEntryKind::File(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn clear(&mut self) -> PackEntry {
        mem::replace(self, PackEntry::new_empty(self.next_block))
    }

    pub fn next_block(&self) -> Option<NonZeroU64> {
        self.next_block
    }

    pub fn set_next_block(&mut self, BlockOffset(nc): BlockOffset) {
        self.next_block = NonZeroU64::new(nc);
    }

    pub fn name(&self) -> Option<&str> {
        match self.kind.as_ref()? {
            PackEntryKind::Directory(DirectoryEntry { name, .. })
            | PackEntryKind::File(FileEntry { name, .. }) => Some(name),
        }
    }

    pub fn name_eq_ignore_ascii_case(&self, other: &str) -> bool {
        self.name().map(|this| this.eq_ignore_ascii_case(other)).unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_none()
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, Some(PackEntryKind::File(_)))
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.kind, Some(PackEntryKind::Directory(_)))
    }
}

impl RawIo for PackEntry {
    /// Reads an entry from the given Read instance always reading exactly
    /// PK2_FILE_ENTRY_SIZE bytes.
    fn from_reader<R: Read>(mut r: R) -> IoResult<Self> {
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
            ty @ (1 | 2) => {
                let name = {
                    let mut buf = [0; 81];
                    r.read_exact(&mut buf)?;
                    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
                    #[cfg(feature = "euc-kr")]
                    let name = encoding_rs::EUC_KR.decode_without_bom_handling(&buf[..end]).0;
                    #[cfg(not(feature = "euc-kr"))]
                    let name = String::from_utf8_lossy(&buf[..end]);
                    name.into_owned().into_boxed_str()
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

                Ok(PackEntry {
                    kind: Some(if ty == 1 {
                        PackEntryKind::Directory(DirectoryEntry {
                            name,
                            access_time,
                            create_time,
                            modify_time,
                            pos_children: ChainIndex(position),
                        })
                    } else {
                        PackEntryKind::File(FileEntry {
                            name,
                            access_time,
                            create_time,
                            modify_time,
                            pos_data: StreamOffset(position),
                            size,
                        })
                    }),
                    next_block,
                })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "archive file is corrupted",
            )),
        }
    }

    fn to_writer<W: Write>(&self, mut w: W) -> IoResult<()> {
        match &self.kind {
            None => {
                w.write_all(
                    &[0; PK2_FILE_ENTRY_SIZE - mem::size_of::<u64>() - mem::size_of::<u16>()],
                )?;
                w.write_u64::<LE>(self.next_block.map_or(0, NonZeroU64::get))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
            Some(
                PackEntryKind::Directory(DirectoryEntry {
                    name,
                    access_time,
                    create_time,
                    modify_time,
                    pos_children: ChainIndex(position),
                })
                | PackEntryKind::File(FileEntry {
                    name,
                    access_time,
                    create_time,
                    modify_time,
                    pos_data: StreamOffset(position),
                    ..
                }),
            ) => {
                w.write_u8(if self.is_dir() { 1 } else { 2 })?;
                #[cfg(feature = "euc-kr")]
                let mut encoded = encoding_rs::EUC_KR.encode(name).0.into_owned();
                #[cfg(not(feature = "euc-kr"))]
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
                w.write_u32::<LE>(match self.kind {
                    Some(PackEntryKind::File(FileEntry { size, .. })) => size,
                    _ => 0,
                })?;
                w.write_u64::<LE>(self.next_block.map_or(0, NonZeroU64::get))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::num::NonZeroU64;

    use crate::constants::{RawPackFileEntry, PK2_FILE_ENTRY_SIZE};
    use crate::filetime::FILETIME;
    use crate::io::RawIo;
    use crate::raw::entry::{DirectoryEntry, FileEntry, PackEntry, PackEntryKind};
    use crate::raw::{ChainIndex, StreamOffset};

    #[test]
    fn pack_entry_read_empty() {
        let mut buf = [0u8; PK2_FILE_ENTRY_SIZE];
        assert_eq!(PackEntry::from_reader(&mut &buf[..]).unwrap(), PackEntry::new_empty(None));
        buf[PK2_FILE_ENTRY_SIZE - 10..][..8].copy_from_slice(&u64::to_le_bytes(1337));

        assert_eq!(
            PackEntry::from_reader(&mut &buf[..]).unwrap(),
            PackEntry::new_empty(NonZeroU64::new(1337))
        );
    }

    #[test]
    fn pack_entry_read_directory() {
        let mut entry = RawPackFileEntry {
            ty: 1,
            name: [0; 81],
            access: FILETIME::default(),
            create: FILETIME::default(),
            modify: FILETIME::default(),
            position: 12345,
            size: 0,
            next_block: 63459,
            _padding: [0, 0],
        };
        entry.name[..6].copy_from_slice(b"foobar");
        assert_eq!(
            PackEntry::from_reader(
                &mut &bytemuck::cast_ref::<_, [u8; PK2_FILE_ENTRY_SIZE]>(&entry)[..]
            )
            .unwrap(),
            PackEntry {
                kind: Some(PackEntryKind::Directory(DirectoryEntry::new_untimed(
                    "foobar".into(),
                    ChainIndex(12345),
                ))),
                next_block: NonZeroU64::new(63459)
            }
        );
    }

    #[test]
    fn pack_entry_read_file() {
        let mut entry = RawPackFileEntry {
            ty: 2,
            name: [0; 81],
            access: FILETIME::default(),
            create: FILETIME::default(),
            modify: FILETIME::default(),
            position: 12345,
            size: 10000,
            next_block: 63459,
            _padding: [0, 0],
        };
        entry.name[..6].copy_from_slice(b"foobar");
        assert_eq!(
            PackEntry::from_reader(
                &mut &bytemuck::cast_ref::<_, [u8; PK2_FILE_ENTRY_SIZE]>(&entry)[..]
            )
            .unwrap(),
            PackEntry {
                kind: Some(PackEntryKind::File(FileEntry::new_untimed(
                    "foobar".into(),
                    StreamOffset(12345),
                    10000,
                ))),
                next_block: NonZeroU64::new(63459)
            }
        );
    }
}
