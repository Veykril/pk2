use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use encoding::{all::WINDOWS_949, DecoderTrap, EncoderTrap, Encoding};

use std::io::{self, Read, Result, Write};
use std::num::NonZeroU64;

use crate::constants::PK2_FILE_ENTRY_SIZE;
use crate::FILETIME;

#[derive(Derivative)]
#[derivative(Debug)]
#[derive(Clone)]
pub enum PackEntry {
    Empty,
    Folder {
        name: String,
        #[derivative(Debug = "ignore")]
        access_time: FILETIME,
        #[derivative(Debug = "ignore")]
        create_time: FILETIME,
        #[derivative(Debug = "ignore")]
        modify_time: FILETIME,
        pos_children: u64,
        next_chain: Option<NonZeroU64>,
    },
    File {
        name: String,
        #[derivative(Debug = "ignore")]
        access_time: FILETIME,
        #[derivative(Debug = "ignore")]
        create_time: FILETIME,
        #[derivative(Debug = "ignore")]
        modify_time: FILETIME,
        pos_data: u64,
        size: u32,
        next_chain: Option<NonZeroU64>,
    },
}

impl Default for PackEntry {
    fn default() -> Self {
        PackEntry::Empty
    }
}

impl PackEntry {
    pub fn new_folder(name: String, pos_children: u64, next_chain: Option<NonZeroU64>) -> Self {
        PackEntry::Folder {
            name,
            access_time: Default::default(),
            create_time: Default::default(),
            modify_time: Default::default(),
            pos_children,
            next_chain,
        }
    }

    pub fn pos_children(&self) -> Option<u64> {
        match *self {
            PackEntry::Folder { pos_children, .. } => Some(pos_children),
            _ => None,
        }
    }

    pub fn next_chain(&self) -> Option<NonZeroU64> {
        match self {
            PackEntry::Empty => None,
            PackEntry::Folder { next_chain, .. } | PackEntry::File { next_chain, .. } => {
                *next_chain
            }
        }
    }

    pub fn set_next_chain(&mut self, nc: u64) {
        match self {
            PackEntry::Empty => (),
            PackEntry::Folder {
                ref mut next_chain, ..
            }
            | PackEntry::File {
                ref mut next_chain, ..
            } => *next_chain = NonZeroU64::new(nc),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            PackEntry::Empty => None,
            PackEntry::Folder { name, .. } | PackEntry::File { name, .. } => Some(name),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            PackEntry::Empty => true,
            _ => false,
        }
    }
}

impl PackEntry {
    // Will always seek to the end of the entry
    pub(crate) fn from_reader<R: Read>(mut r: R) -> Result<Self> {
        match r.read_u8()? {
            0 => {
                r.read_exact(&mut [0; PK2_FILE_ENTRY_SIZE - 1])?; //seek to end of entry
                Ok(PackEntry::Empty)
            }
            ty @ 1 | ty @ 2 => {
                let name = {
                    let mut buf = [0; 81];
                    r.read_exact(&mut buf)?;
                    let end = buf
                        .iter()
                        .position(|b| *b == 0)
                        .unwrap_or_else(|| buf.len());
                    WINDOWS_949
                        .decode(&buf[..end], DecoderTrap::Replace)
                        .unwrap() //todo replace unwrap and make use of the decoder trap
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
                    PackEntry::Folder {
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
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown PackFileEntry type",
            )),
        }
    }

    pub fn to_writer<W: Write>(&self, mut w: W) -> Result<()> {
        match self {
            PackEntry::Empty => w.write_all(&[0; PK2_FILE_ENTRY_SIZE]),
            PackEntry::Folder {
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
                let mut encoded = WINDOWS_949.encode(name, EncoderTrap::Strict).unwrap();
                encoded.resize(80, 0);
                w.write_all(&encoded)?;
                w.write_u8(0)?;
                w.write_u32::<LE>(access_time.dwLowDateTime)?;
                w.write_u32::<LE>(access_time.dwHighDateTime)?;
                w.write_u32::<LE>(create_time.dwLowDateTime)?;
                w.write_u32::<LE>(create_time.dwHighDateTime)?;
                w.write_u32::<LE>(modify_time.dwLowDateTime)?;
                w.write_u32::<LE>(modify_time.dwHighDateTime)?;
                w.write_u64::<LE>(*position)?;
                w.write_u32::<LE>(match self {
                    PackEntry::Folder { .. } => 0,
                    PackEntry::File { size, .. } => *size,
                    PackEntry::Empty => unreachable!(),
                })?;
                w.write_u64::<LE>(next_chain.map_or(0, |nc| nc.get()))?;
                Ok(())
            }
        }
    }
}
