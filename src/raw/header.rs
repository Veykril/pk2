use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};

use std::fmt;
use std::io::{self, Read, Write};

use crate::constants::*;
use crate::error::{Error, Pk2Result};
use crate::Blowfish;

pub struct PackHeader {
    pub signature: [u8; 30],
    pub version: u32,
    pub encrypted: bool,
    pub verify: [u8; 16],
    pub reserved: [u8; 205],
}

impl Default for PackHeader {
    fn default() -> Self {
        PackHeader {
            signature: *PK2_SIGNATURE,
            version: PK2_VERSION,
            encrypted: false,
            verify: *PK2_CHECKSUM,
            reserved: [0; 205],
        }
    }
}

impl PackHeader {
    pub fn new_encrypted(bf: &Blowfish) -> Self {
        let mut this = Self::default();
        let _ = bf.encrypt(&mut this.verify);
        this.encrypted = true;
        this
    }

    pub fn new() -> Self {
        Default::default()
    }

    /// Validate the signature of this header. Returns an error if the version
    /// or signature does not match.
    pub fn validate_sig(&self) -> Pk2Result<()> {
        if &self.signature != PK2_SIGNATURE {
            Err(Error::CorruptedFile)
        } else if self.version != PK2_VERSION {
            Err(Error::UnsupportedVersion)
        } else {
            Ok(())
        }
    }

    /// Verifies the calculated checksum against this header returning an error
    /// if it doesn't match.
    pub fn verify(&self, checksum: [u8; 16]) -> Pk2Result<()> {
        if checksum[..PK2_CHECKSUM_STORED] != self.verify[..PK2_CHECKSUM_STORED] {
            Err(Error::InvalidKey)
        } else {
            Ok(())
        }
    }

    pub fn from_reader<R: Read>(mut r: R) -> io::Result<Self> {
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

    pub fn to_writer<W: Write>(&self, mut w: W) -> io::Result<()> {
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
