use alloc::boxed::Box;
use alloc::string::String;
use core::num::NonZeroU64;
use core::{fmt, mem};

use crate::filetime::FILETIME;
use crate::format::{BlockOffset, ChainOffset, StreamOffset};
use crate::parse::{read_le_u8, read_le_u16, read_le_u32, read_le_u64};

/// The structure of a single entry in a pack file.
#[repr(C, packed)]
#[derive(Copy, Clone)]
struct RawPackFileEntry {
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

impl RawPackFileEntry {
    const TY_EMPTY: u8 = 0;
    const TY_DIRECTORY: u8 = 1;
    const TY_FILE: u8 = 2;
}

/// An entry of a [`PackBlock`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackEntry {
    entry: Option<NonEmptyEntry>,
    next_block: Option<BlockOffset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonEmptyEntry {
    kind: DirectoryOrFile,
    name: Box<str>,
    pub access_time: FILETIME,
    pub create_time: FILETIME,
    pub modify_time: FILETIME,
}

impl NonEmptyEntry {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: &str) -> Result<(), ()> {
        if name.len() > 81 {
            return Err(());
        }
        self.name = Box::from(name);
        Ok(())
    }

    pub fn is_directory(&self) -> bool {
        matches!(self.kind, DirectoryOrFile::Directory { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, DirectoryOrFile::File { .. })
    }

    pub fn directory_children_offset(&self) -> Option<ChainOffset> {
        match self.kind {
            DirectoryOrFile::Directory { pos_children } => Some(pos_children),
            _ => None,
        }
    }

    pub fn file_data(&self) -> Option<(StreamOffset, u32)> {
        match self.kind {
            DirectoryOrFile::File { pos_data, size } => Some((pos_data, size)),
            _ => None,
        }
    }

    pub fn set_file_data(&mut self, pos_data: StreamOffset, size: u32) -> Result<(), ()> {
        match &mut self.kind {
            DirectoryOrFile::File { pos_data: pos_data_tgt, size: size_tgt } => {
                *pos_data_tgt = pos_data;
                *size_tgt = size;
                Ok(())
            }
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryOrFile {
    Directory { pos_children: ChainOffset },
    File { pos_data: StreamOffset, size: u32 },
}

impl PackEntry {
    pub fn new_directory(
        name: impl Into<Box<str>>,
        pos_children: ChainOffset,
        next_block: Option<BlockOffset>,
    ) -> Self {
        PackEntry {
            entry: Some(NonEmptyEntry {
                kind: DirectoryOrFile::Directory { pos_children },
                name: name.into(),
                access_time: FILETIME::default(),
                create_time: FILETIME::default(),
                modify_time: FILETIME::default(),
            }),
            next_block,
        }
    }

    pub fn new_file(
        name: impl Into<Box<str>>,
        pos_data: StreamOffset,
        size: u32,
        next_block: Option<BlockOffset>,
    ) -> Self {
        PackEntry {
            entry: Some(NonEmptyEntry {
                kind: DirectoryOrFile::File { pos_data, size },
                name: name.into(),
                access_time: FILETIME::default(),
                create_time: FILETIME::default(),
                modify_time: FILETIME::default(),
            }),
            next_block,
        }
    }

    pub fn new_empty(next_block: Option<BlockOffset>) -> Self {
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

    pub fn children(&self) -> Option<ChainOffset> {
        match self.entry {
            Some(NonEmptyEntry { kind: DirectoryOrFile::Directory { pos_children }, .. }) => {
                Some(pos_children)
            }
            _ => None,
        }
    }

    pub fn next_block(&self) -> Option<BlockOffset> {
        self.next_block
    }

    pub fn set_next_block(&mut self, nb: BlockOffset) {
        self.next_block = Some(nb);
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

#[derive(Clone, Copy, Debug)]
pub struct InvalidPackEntryType(pub u8);

impl fmt::Display for InvalidPackEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid pack entry type: {:#x}", self.0)
    }
}

impl PackEntry {
    pub const PK2_FILE_ENTRY_SIZE: usize = size_of::<RawPackFileEntry>();

    pub fn parse(buffer: &[u8; Self::PK2_FILE_ENTRY_SIZE]) -> Result<Self, InvalidPackEntryType> {
        let buffer = &mut &buffer[..];
        match read_le_u8(buffer).unwrap() {
            RawPackFileEntry::TY_EMPTY => {
                *buffer = &buffer[Self::PK2_FILE_ENTRY_SIZE
                    - size_of::<u64>()
                    - size_of::<u16>()
                    - size_of::<u8>()..];
                let next_block = NonZeroU64::new(read_le_u64(buffer).unwrap());

                *buffer = &buffer[size_of::<u16>()..];
                Ok(PackEntry::new_empty(next_block.map(BlockOffset)))
            }
            ty @ (RawPackFileEntry::TY_DIRECTORY | RawPackFileEntry::TY_FILE) => {
                let name = {
                    let s;
                    (s, *buffer) = buffer.split_at(81);
                    let end = s.iter().position(|b| *b == 0).unwrap_or(s.len());
                    let s = &s[..end];
                    #[cfg(feature = "euc-kr")]
                    let name = encoding_rs::EUC_KR.decode_without_bom_handling(s).0;
                    #[cfg(not(feature = "euc-kr"))]
                    let name = String::from_utf8_lossy(s);
                    name.into_owned().into_boxed_str()
                };
                let access_time = FILETIME {
                    dwLowDateTime: read_le_u32(buffer).unwrap(),
                    dwHighDateTime: read_le_u32(buffer).unwrap(),
                };
                let create_time = FILETIME {
                    dwLowDateTime: read_le_u32(buffer).unwrap(),
                    dwHighDateTime: read_le_u32(buffer).unwrap(),
                };
                let modify_time = FILETIME {
                    dwLowDateTime: read_le_u32(buffer).unwrap(),
                    dwHighDateTime: read_le_u32(buffer).unwrap(),
                };
                let position = read_le_u64(buffer).unwrap();
                let size = read_le_u32(buffer).unwrap();
                let next_block = NonZeroU64::new(read_le_u64(buffer).unwrap());
                read_le_u16(buffer).unwrap(); //padding

                Ok(PackEntry {
                    entry: Some(NonEmptyEntry {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        kind: if ty == RawPackFileEntry::TY_DIRECTORY {
                            DirectoryOrFile::Directory {
                                pos_children: ChainOffset(
                                    // FIXME: Error type
                                    NonZeroU64::new(position).ok_or(InvalidPackEntryType(ty))?,
                                ),
                            }
                        } else {
                            DirectoryOrFile::File {
                                pos_data: StreamOffset(
                                    // FIXME: Error type
                                    NonZeroU64::new(position).ok_or(InvalidPackEntryType(ty))?,
                                ),
                                size,
                            }
                        },
                    }),
                    next_block: next_block.map(BlockOffset),
                })
            }
            ty => Err(InvalidPackEntryType(ty)),
        }
    }

    pub fn write_to(&self, buffer: &mut [u8; Self::PK2_FILE_ENTRY_SIZE]) {
        let buffer = &mut buffer[..];
        match &self.entry {
            Some(entry) => {
                buffer[0] = match entry.kind {
                    DirectoryOrFile::Directory { .. } => RawPackFileEntry::TY_DIRECTORY,
                    DirectoryOrFile::File { .. } => RawPackFileEntry::TY_FILE,
                };
                #[cfg(feature = "euc-kr")]
                let name = &encoding_rs::EUC_KR.encode(&entry.name).0;
                #[cfg(not(feature = "euc-kr"))]
                let name = entry.name.as_bytes();
                buffer[1..][..name.len().min(80)].copy_from_slice(&name[..name.len().min(80)]);
                buffer[81] = 0;
                buffer[82..86].copy_from_slice(&entry.access_time.dwLowDateTime.to_le_bytes());
                buffer[86..90].copy_from_slice(&entry.access_time.dwHighDateTime.to_le_bytes());
                buffer[90..94].copy_from_slice(&entry.create_time.dwLowDateTime.to_le_bytes());
                buffer[94..98].copy_from_slice(&entry.create_time.dwHighDateTime.to_le_bytes());
                buffer[98..102].copy_from_slice(&entry.modify_time.dwLowDateTime.to_le_bytes());
                buffer[102..106].copy_from_slice(&entry.modify_time.dwHighDateTime.to_le_bytes());
                match entry.kind {
                    DirectoryOrFile::Directory { pos_children } => {
                        buffer[106..114].copy_from_slice(&pos_children.0.get().to_le_bytes());
                        buffer[114..118].copy_from_slice(&0u32.to_le_bytes());
                    }
                    DirectoryOrFile::File { pos_data, size } => {
                        buffer[106..114].copy_from_slice(&pos_data.0.get().to_le_bytes());
                        buffer[114..118].copy_from_slice(&size.to_le_bytes());
                    }
                }
            }
            None => {
                buffer[0] = RawPackFileEntry::TY_EMPTY;
                buffer[106..114].copy_from_slice(&0u64.to_le_bytes());
                buffer[114..118].copy_from_slice(&0u32.to_le_bytes());
            }
        }
        buffer[118..126].copy_from_slice(&self.next_block.map_or(0, |b| b.0.get()).to_le_bytes());
        buffer[126..128].copy_from_slice(&0u16.to_le_bytes());
    }
}

#[cfg(test)]
mod test {
    use core::num::NonZeroU64;

    use crate::BlockOffset;
    use crate::filetime::FILETIME;
    use crate::format::entry::{DirectoryOrFile, NonEmptyEntry, PackEntry, RawPackFileEntry};
    use crate::format::{ChainOffset, StreamOffset};

    unsafe impl bytemuck::Pod for RawPackFileEntry {}
    unsafe impl bytemuck::Zeroable for RawPackFileEntry {}

    #[test]
    fn pack_entry_read_empty() {
        let mut buf = [0u8; PackEntry::PK2_FILE_ENTRY_SIZE];
        assert_eq!(PackEntry::parse(&buf).unwrap(), PackEntry::new_empty(None));
        buf[PackEntry::PK2_FILE_ENTRY_SIZE - 10..][..8].copy_from_slice(&u64::to_le_bytes(1337));

        assert_eq!(
            PackEntry::parse(&buf).unwrap(),
            PackEntry::new_empty(NonZeroU64::new(1337).map(BlockOffset))
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
            PackEntry::parse(bytemuck::cast_ref::<_, [u8; PackEntry::PK2_FILE_ENTRY_SIZE]>(&entry))
                .unwrap(),
            PackEntry {
                entry: Some(NonEmptyEntry {
                    kind: DirectoryOrFile::Directory {
                        pos_children: NonZeroU64::new(12345).map(ChainOffset).unwrap()
                    },
                    name: "foobar".into(),
                    access_time: FILETIME::default(),
                    create_time: FILETIME::default(),
                    modify_time: FILETIME::default(),
                }),
                next_block: NonZeroU64::new(63459).map(BlockOffset)
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
            PackEntry::parse(bytemuck::cast_ref::<_, [u8; PackEntry::PK2_FILE_ENTRY_SIZE]>(&entry))
                .unwrap(),
            PackEntry {
                entry: Some(NonEmptyEntry {
                    kind: DirectoryOrFile::File {
                        pos_data: StreamOffset(NonZeroU64::new(12345).unwrap()),
                        size: 10000
                    },
                    name: "foobar".into(),
                    access_time: FILETIME::default(),
                    create_time: FILETIME::default(),
                    modify_time: FILETIME::default(),
                }),
                next_block: NonZeroU64::new(63459).map(BlockOffset)
            }
        );
    }
}
