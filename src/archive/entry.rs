use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use encoding_rs::EUC_KR;

use std::io::{self, Read, Result, Write};
use std::num::NonZeroU64;

use crate::constants::PK2_FILE_ENTRY_SIZE;
use crate::FILETIME;

#[derive(Clone, Eq, PartialEq)]
pub(in crate) enum PackEntry {
    Empty {
        next_chain: Option<NonZeroU64>,
    },
    Directory {
        name: String,
        access_time: FILETIME,
        create_time: FILETIME,
        modify_time: FILETIME,
        pos_children: u64,
        next_chain: Option<NonZeroU64>,
    },
    File {
        name: String,
        access_time: FILETIME,
        create_time: FILETIME,
        modify_time: FILETIME,
        pos_data: u64,
        size: u32,
        next_chain: Option<NonZeroU64>,
    },
}

impl Default for PackEntry {
    fn default() -> Self {
        PackEntry::Empty { next_chain: None }
    }
}

impl PackEntry {
    pub fn new_directory(name: String, pos_children: u64, next_chain: Option<NonZeroU64>) -> Self {
        PackEntry::Directory {
            name,
            access_time: Default::default(),
            create_time: Default::default(),
            modify_time: Default::default(),
            pos_children,
            next_chain,
        }
    }

    pub fn new_file(
        name: String,
        pos_data: u64,
        size: u32,
        next_chain: Option<NonZeroU64>,
    ) -> Self {
        PackEntry::File {
            name,
            access_time: Default::default(),
            create_time: Default::default(),
            modify_time: Default::default(),
            pos_data,
            size,
            next_chain,
        }
    }

    pub fn pos_children(&self) -> Option<u64> {
        match *self {
            PackEntry::Directory { pos_children, .. } => Some(pos_children),
            _ => None,
        }
    }

    pub fn next_chain(&self) -> Option<NonZeroU64> {
        match *self {
            PackEntry::Empty { next_chain }
            | PackEntry::Directory { next_chain, .. }
            | PackEntry::File { next_chain, .. } => next_chain,
        }
    }

    pub fn set_next_chain(&mut self, nc: u64) {
        match self {
            PackEntry::Empty { .. } => (),
            PackEntry::Directory {
                ref mut next_chain, ..
            }
            | PackEntry::File {
                ref mut next_chain, ..
            } => *next_chain = NonZeroU64::new(nc),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            PackEntry::Empty { .. } => None,
            PackEntry::Directory { name, .. } | PackEntry::File { name, .. } => Some(name),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            PackEntry::Empty { .. } => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match self {
            PackEntry::File { .. } => true,
            _ => false,
        }
    }

    pub fn is_dir(&self) -> bool {
        match self {
            PackEntry::Directory { .. } => true,
            _ => false,
        }
    }
}

impl PackEntry {
    // Will always seek to the end of the entry
    pub(in crate) fn from_reader<R: Read>(mut r: R) -> Result<Self> {
        match r.read_u8()? {
            0 => {
                r.read_exact(&mut [0; PK2_FILE_ENTRY_SIZE - 1])?; //seek to end of entry
                Ok(PackEntry::Empty { next_chain: None })
            }
            ty @ 1 | ty @ 2 => {
                let name = {
                    let mut buf = [0; 81];
                    r.read_exact(&mut buf)?;
                    let end = buf
                        .iter()
                        .position(|b| *b == 0)
                        .unwrap_or_else(|| buf.len());
                    EUC_KR
                        .decode_without_bom_handling(&buf[..end])
                        .0
                        .into_owned()
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
                let next_chain = NonZeroU64::new(r.read_u64::<LE>()?);
                r.read_u16::<LE>()?; //padding

                Ok(if ty == 1 {
                    PackEntry::Directory {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        pos_children: position,
                        next_chain,
                    }
                } else {
                    PackEntry::File {
                        name,
                        access_time,
                        create_time,
                        modify_time,
                        pos_data: position,
                        size,
                        next_chain,
                    }
                })
            }
            ty => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown PackFileEntry type {}", ty),
            )),
        }
    }

    pub fn to_writer<W: Write>(&self, mut w: W) -> Result<()> {
        match self {
            PackEntry::Empty { next_chain } => {
                w.write_all(&[0; PK2_FILE_ENTRY_SIZE - 8])?;
                w.write_u64::<LE>(next_chain.map_or(0, |nc| nc.get()))
            }
            PackEntry::Directory {
                name,
                access_time,
                create_time,
                modify_time,
                pos_children: position,
                next_chain,
            }
            | PackEntry::File {
                name,
                access_time,
                create_time,
                modify_time,
                pos_data: position,
                next_chain,
                ..
            } => {
                w.write_u8(if self.is_dir() { 1 } else { 2 })?;
                let mut encoded = EUC_KR.encode(name).0.into_owned();
                encoded.resize(81, 0);
                w.write_all(&encoded)?;
                w.write_u32::<LE>(access_time.dwLowDateTime)?;
                w.write_u32::<LE>(access_time.dwHighDateTime)?;
                w.write_u32::<LE>(create_time.dwLowDateTime)?;
                w.write_u32::<LE>(create_time.dwHighDateTime)?;
                w.write_u32::<LE>(modify_time.dwLowDateTime)?;
                w.write_u32::<LE>(modify_time.dwHighDateTime)?;
                w.write_u64::<LE>(*position)?;
                w.write_u32::<LE>(match self {
                    PackEntry::Directory { .. } => 0,
                    PackEntry::File { size, .. } => *size,
                    _ => unreachable!(),
                })?;
                w.write_u64::<LE>(next_chain.map_or(0, |nc| nc.get()))?;
                w.write_u16::<LE>(0)?;
                Ok(())
            }
        }
    }
}
