use block_modes::BlockMode;
use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};

use std::fmt;
use std::io::{self, Read, Write};

use crate::constants;
use crate::Blowfish;

pub(crate) struct PackHeader {
    pub signature: [u8; 30],
    pub version: u32,
    pub encrypted: bool,
    pub verify: [u8; 16],
    pub reserved: [u8; 205],
}

impl Default for PackHeader {
    fn default() -> Self {
        PackHeader {
            signature: *constants::PK2_SIGNATURE,
            version: constants::PK2_VERSION,
            encrypted: false,
            verify: *constants::PK2_CHECKSUM,
            reserved: [0; 205],
        }
    }
}

impl PackHeader {
    pub(in crate) fn new_encrypted(bf: &mut Blowfish) -> Self {
        let mut this = Self::default();
        let _ = bf.encrypt_nopad(&mut this.verify);
        this.encrypted = true;
        this
    }

    pub(in crate) fn from_reader<R: Read>(mut r: R) -> io::Result<Self> {
        let mut signature = [0; 30];
        r.read_exact(&mut signature)?;
        let version = r.read_u32::<LE>()?;
        let encrypted = r.read_u8()? != 0;
        let mut verify = [0; 16];
        r.read_exact(&mut verify)?;
        let mut reserved = [0; 205];
        r.read_exact(&mut reserved)?;
        Ok(PackHeader {
            signature,
            version,
            encrypted,
            verify,
            reserved,
        })
    }

    pub(in crate) fn to_writer<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_all(&self.signature)?;
        w.write_u32::<LE>(self.version)?;
        w.write_u8(self.encrypted as u8)?;
        w.write_all(&self.verify[..3])?;
        w.write_all(&[0; 16 - 3])?;
        w.write_all(&self.reserved)?;
        Ok(())
    }
}

impl fmt::Debug for PackHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::ffi::CStr;
        unsafe {
            f.debug_struct("PackHeader")
                .field("signature", &CStr::from_ptr(self.signature.as_ptr() as _))
                .field("version", &self.version)
                .field("encrypted", &self.encrypted)
                .field("verify", &CStr::from_ptr(self.verify.as_ptr() as _))
                .field("reserved", &"\"omitted\"")
                .finish()
        }
    }
}
