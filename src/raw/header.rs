use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};

use std::fmt;
use std::io::{Read, Result as IoResult, Write};

use crate::constants::*;
use crate::error::{OpenError, OpenResult};
use crate::io::RawIo;
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
    pub fn validate_sig(&self) -> OpenResult<()> {
        if &self.signature != PK2_SIGNATURE {
            Err(OpenError::CorruptedFile)
        } else if self.version != PK2_VERSION {
            Err(OpenError::UnsupportedVersion)
        } else {
            Ok(())
        }
    }

    /// Verifies the calculated checksum against this header returning an error
    /// if it doesn't match.
    pub fn verify(&self, checksum: [u8; 16]) -> OpenResult<()> {
        if checksum[..PK2_CHECKSUM_STORED] != self.verify[..PK2_CHECKSUM_STORED] {
            Err(OpenError::InvalidKey)
        } else {
            Ok(())
        }
    }
}

impl RawIo for PackHeader {
    fn from_reader<R: Read>(mut r: R) -> IoResult<Self> {
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

    fn to_writer<W: Write>(&self, mut w: W) -> IoResult<()> {
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
        let sig_end = self
            .signature
            .iter()
            .position(|&b| b == 0)
            .unwrap_or_else(|| self.signature.len());
        f.debug_struct("PackHeader")
            .field(
                "signature",
                &std::str::from_utf8(&self.signature[..sig_end]),
            )
            .field("version", &self.version)
            .field("encrypted", &self.encrypted)
            .field("verify", &"\"omitted\"")
            .field("reserved", &"\"omitted\"")
            .finish()
    }
}
