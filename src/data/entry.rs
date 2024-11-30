use byteorder::{ReadBytesExt, WriteBytesExt, LE};

use std::io::{Read, Result as IoResult, Write};
use std::mem;
use std::num::NonZeroU64;
use std::time::SystemTime;

use crate::constants::{
    RawPackFileEntry, PK2_CURRENT_DIR_IDENT, PK2_FILE_ENTRY_SIZE, PK2_PARENT_DIR_IDENT,
};
use crate::data::{BlockOffset, ChainIndex, StreamOffset};
use crate::filetime::FILETIME;
use crate::io::RawIo;

/// An entry of a [`PackBlock`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackEntry {
    pub(crate) entry: Option<NonEmptyEntry>,
    next_block: Option<NonZeroU64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonEmptyEntry {
    pub(crate) kind: DirectoryOrFile,
    name: Box<str>,
    pub(crate) access_time: FILETIME,
    pub(crate) create_time: FILETIME,
    pub(crate) modify_time: FILETIME,
}

impl NonEmptyEntry {
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

    pub fn is_current_link(&self) -> bool {
        self.name() == PK2_CURRENT_DIR_IDENT
    }

    pub fn is_parent_link(&self) -> bool {
        self.name() == PK2_PARENT_DIR_IDENT
    }

    pub fn is_normal_link(&self) -> bool {
        !(self.is_current_link() || self.is_parent_link())
    }

    pub fn is_directory(&self) -> bool {
        matches!(self.kind, DirectoryOrFile::Directory { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, DirectoryOrFile::File { .. })
    }

    pub fn directory_children_position(&self) -> Option<ChainIndex> {
        match self.kind {
            DirectoryOrFile::Directory { pos_children } => Some(pos_children),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryOrFile {
    Directory { pos_children: ChainIndex },
    File { pos_data: StreamOffset, size: u32 },
}

impl PackEntry {
    pub fn new_directory(
        name: impl Into<Box<str>>,
        pos_children: ChainIndex,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        let now = FILETIME::now();
        PackEntry {
            entry: Some(NonEmptyEntry {
                kind: DirectoryOrFile::Directory { pos_children },
                name: name.into(),
                access_time: now,
                create_time: now,
                modify_time: now,
            }),
            next_block,
        }
    }

    pub fn new_file(
        name: impl Into<Box<str>>,
        pos_data: StreamOffset,
        size: u32,
        next_block: Option<NonZeroU64>,
    ) -> Self {
        let now = FILETIME::now();
        PackEntry {
            entry: Some(NonEmptyEntry {
                kind: DirectoryOrFile::File { pos_data, size },
                name: name.into(),
                access_time: now,
                create_time: now,
                modify_time: now,
            }),
            next_block,
        }
    }

    pub fn new_empty(next_block: Option<NonZeroU64>) -> Self {
        PackEntry { entry: None, next_block }
    }

    pub fn as_non_empty(&self) -> Option<&NonEmptyEntry> {
        self.entry.as_ref()
    }

    pub fn as_non_empty_mut(&mut self) -> Option<&mut NonEmptyEntry> {
        self.entry.as_mut()
    }

    pub fn is_directory(&self) -> bool {
        matches!(self.entry, Some(NonEmptyEntry { kind: DirectoryOrFile::Directory { .. }, .. }))
    }

    pub fn is_file(&self) -> bool {
        matches!(self.entry, Some(NonEmptyEntry { kind: DirectoryOrFile::File { .. }, .. }))
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
        Some(self.entry.as_ref()?.name())
    }

    pub fn name_eq_ignore_ascii_case(&self, other: &str) -> bool {
        self.name().map(|this| this.eq_ignore_ascii_case(other)).unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.entry.is_none()
    }
}

impl RawIo for PackEntry {
    /// Reads an entry from the given Read instance always reading exactly
    /// PK2_FILE_ENTRY_SIZE bytes.
    fn from_reader<R: Read>(mut r: R) -> IoResult<Self> {
        match r.read_u8()? {
            RawPackFileEntry::TY_EMPTY => {
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
            ty @ (RawPackFileEntry::TY_DIRECTORY | RawPackFileEntry::TY_FILE) => {
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
                    entry: Some(NonEmptyEntry {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        kind: if ty == RawPackFileEntry::TY_DIRECTORY {
                            DirectoryOrFile::Directory { pos_children: ChainIndex(position) }
                        } else {
                            DirectoryOrFile::File { pos_data: StreamOffset(position), size }
                        },
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
        match &self.entry {
            None => {
                w.write_all(
                    &[0; PK2_FILE_ENTRY_SIZE - mem::size_of::<u64>() - mem::size_of::<u16>()],
                )?;
                w.write_u64::<LE>(self.next_block.map_or(0, NonZeroU64::get))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
            Some(NonEmptyEntry {
                kind:
                    DirectoryOrFile::Directory { pos_children: ChainIndex(position) }
                    | DirectoryOrFile::File { pos_data: StreamOffset(position), .. },
                name,
                access_time,
                create_time,
                modify_time,
            }) => {
                w.write_u8(if self.is_directory() {
                    RawPackFileEntry::TY_DIRECTORY
                } else {
                    RawPackFileEntry::TY_FILE
                })?;
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
                w.write_u32::<LE>(match self.entry {
                    Some(NonEmptyEntry { kind: DirectoryOrFile::File { size, .. }, .. }) => size,
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
    use crate::data::entry::{DirectoryOrFile, NonEmptyEntry, PackEntry};
    use crate::data::{ChainIndex, StreamOffset};
    use crate::filetime::FILETIME;
    use crate::io::RawIo;

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
            ty: RawPackFileEntry::TY_DIRECTORY,
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
                entry: Some(NonEmptyEntry {
                    kind: DirectoryOrFile::Directory { pos_children: ChainIndex(12345) },
                    name: "foobar".into(),
                    access_time: FILETIME::default(),
                    create_time: FILETIME::default(),
                    modify_time: FILETIME::default(),
                }),
                next_block: NonZeroU64::new(63459)
            }
        );
    }

    #[test]
    fn pack_entry_read_file() {
        let mut entry = RawPackFileEntry {
            ty: RawPackFileEntry::TY_FILE,
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
                entry: Some(NonEmptyEntry {
                    kind: DirectoryOrFile::File { pos_data: StreamOffset(12345), size: 10000 },
                    name: "foobar".into(),
                    access_time: FILETIME::default(),
                    create_time: FILETIME::default(),
                    modify_time: FILETIME::default(),
                }),
                next_block: NonZeroU64::new(63459)
            }
        );
    }
}
