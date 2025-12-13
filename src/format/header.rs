use alloc::fmt;

use crate::InvalidKey;
use crate::blowfish::Blowfish;
use crate::error::{HeaderError, HeaderResult};

const PK2_VERSION: u32 = 0x0100_0002;
const PK2_SIGNATURE: &[u8; 30] = b"JoyMax File Manager!\n\0\0\0\0\0\0\0\0\0";
/// The number of bytes in the checksum that are actually stored in the header. Yes, the archive
/// only stores 3 bytes of the checksum...
const PK2_CHECKSUM_STORED: usize = 3;
/// The checksum value.
const PK2_CHECKSUM: &[u8; 16] = b"Joymax Pak File\0";

/// The in-file header layout.
#[repr(C, packed)]
struct CPackHeader {
    signature: [u8; 30],
    version: u32,
    encrypted: u8,
    verify: [u8; 16],
    reserved: [u8; 205],
}

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
    pub const PACK_HEADER_LEN: usize = size_of::<CPackHeader>();

    pub fn new_encrypted(bf: &Blowfish) -> Self {
        let mut this = Self::default();
        bf.encrypt(&mut this.verify);
        this.encrypted = true;
        this
    }

    /// Validate the signature of this header. Returns an error if the version
    /// or signature does not match.
    pub fn validate_sig(&self) -> HeaderResult<()> {
        if &self.signature != PK2_SIGNATURE {
            Err(HeaderError::CorruptedFile)
        } else if self.version != PK2_VERSION {
            Err(HeaderError::UnsupportedVersion(self.version))
        } else {
            Ok(())
        }
    }

    /// Verifies the calculated checksum against this header returning an error
    /// if it doesn't match.
    pub fn verify(&self, bf: &Blowfish) -> Result<(), InvalidKey> {
        let mut checksum = *PK2_CHECKSUM;
        bf.encrypt(&mut checksum);
        if checksum[..PK2_CHECKSUM_STORED] != self.verify[..PK2_CHECKSUM_STORED] {
            Err(InvalidKey)
        } else {
            Ok(())
        }
    }
}

impl PackHeader {
    pub fn parse(buffer: &[u8; Self::PACK_HEADER_LEN]) -> Self {
        let (signature, buffer) = buffer.split_at(30);
        let (version, buffer) = buffer.split_at(4);
        let version = u32::from_le_bytes((*version).try_into().unwrap());
        let (encrypted, buffer) = buffer.split_at(1);
        let encrypted = encrypted[0] != 0;
        let (verify, buffer) = buffer.split_at(16);
        let (reserved, buffer) = buffer.split_at(205);
        assert!(buffer.is_empty());
        Self {
            signature: (*signature).try_into().unwrap(),
            version,
            encrypted,
            verify: (*verify).try_into().unwrap(),
            reserved: (*reserved).try_into().unwrap(),
        }
    }

    pub fn write_into(&self, buffer: &mut [u8; Self::PACK_HEADER_LEN]) {
        let (signature, buffer) = buffer.split_at_mut(30);
        let (version, buffer) = buffer.split_at_mut(4);
        let (encrypted, buffer) = buffer.split_at_mut(1);
        let (verify, reserved) = buffer.split_at_mut(16);
        signature.copy_from_slice(&self.signature);
        version.copy_from_slice(&self.version.to_le_bytes());
        encrypted[0] = self.encrypted as u8;
        verify.copy_from_slice(&self.verify);
        reserved.copy_from_slice(&self.reserved);
    }
}

impl fmt::Debug for PackHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let sig_end = self.signature.iter().position(|&b| b == 0).unwrap_or(self.signature.len());
        f.debug_struct("PackHeader")
            .field("signature", &alloc::str::from_utf8(&self.signature[..sig_end]))
            .field("version", &self.version)
            .field("encrypted", &self.encrypted)
            .field("verify", &"\"...\"")
            .field("reserved", &"\"...\"")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_header() {
        let header = PackHeader::default();
        assert_eq!(&header.signature, PK2_SIGNATURE);
        assert_eq!(header.version, PK2_VERSION);
        assert_eq!(&header.verify, PK2_CHECKSUM);
        assert!(!header.encrypted);
        assert!(header.validate_sig().is_ok());
    }

    #[test]
    fn header_with_invalid_signature_fails_validation() {
        let mut header = PackHeader::default();
        header.signature[0] = b'X';
        assert!(matches!(header.validate_sig(), Err(HeaderError::CorruptedFile)));
    }

    #[test]
    fn header_with_invalid_version_fails_validation() {
        let header = PackHeader { version: 0x9999, ..Default::default() };
        assert!(matches!(header.validate_sig(), Err(HeaderError::UnsupportedVersion(0x9999))));
    }

    #[test]
    fn parse_write_roundtrip() {
        let original = PackHeader::default();
        let mut buffer = [0u8; PackHeader::PACK_HEADER_LEN];
        original.write_into(&mut buffer);

        let parsed = PackHeader::parse(&buffer);

        assert_eq!(parsed.signature, original.signature);
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.encrypted, original.encrypted);
        assert_eq!(parsed.verify, original.verify);
        assert_eq!(parsed.reserved, original.reserved);
    }

    #[test]
    fn encrypted_header_roundtrip() {
        let bf = Blowfish::new(b"testkey").unwrap();
        let original = PackHeader::new_encrypted(&bf);

        assert!(original.encrypted);

        let mut buffer = [0u8; PackHeader::PACK_HEADER_LEN];
        original.write_into(&mut buffer);

        let parsed = PackHeader::parse(&buffer);

        assert_eq!(parsed.signature, original.signature);
        assert_eq!(parsed.version, original.version);
        assert!(parsed.encrypted);
        assert_eq!(parsed.verify, original.verify);
    }

    #[test]
    fn verify_with_correct_key_succeeds() {
        let bf = Blowfish::new(b"testkey").unwrap();
        let header = PackHeader::new_encrypted(&bf);
        assert!(header.verify(&bf).is_ok());
    }

    #[test]
    fn verify_with_wrong_key_fails() {
        let bf_encrypt = Blowfish::new(b"correctkey").unwrap();
        let bf_wrong = Blowfish::new(b"wrongkey").unwrap();
        let header = PackHeader::new_encrypted(&bf_encrypt);
        assert!(header.verify(&bf_wrong).is_err());
    }

    #[test]
    fn verify_unencrypted_header_with_any_key() {
        // Unencrypted header has plaintext checksum
        // Verifying with any key should fail since the checksum isn't encrypted
        let header = PackHeader::default();
        let bf = Blowfish::new(b"anykey").unwrap();
        // The verify method encrypts the checksum and compares, so it will fail
        // for an unencrypted header unless the key happens to produce matching bytes
        assert!(header.verify(&bf).is_err());
    }

    #[test]
    fn write_then_parse_preserves_encrypted_flag() {
        let header = PackHeader { encrypted: true, ..Default::default() };

        let mut buffer = [0u8; PackHeader::PACK_HEADER_LEN];
        header.write_into(&mut buffer);

        let parsed = PackHeader::parse(&buffer);
        assert!(parsed.encrypted);
    }

    #[test]
    fn write_then_parse_preserves_reserved_bytes() {
        let mut header = PackHeader::default();
        header.reserved[0] = 0xAB;
        header.reserved[100] = 0xCD;
        header.reserved[204] = 0xEF;

        let mut buffer = [0u8; PackHeader::PACK_HEADER_LEN];
        header.write_into(&mut buffer);

        let parsed = PackHeader::parse(&buffer);
        assert_eq!(parsed.reserved[0], 0xAB);
        assert_eq!(parsed.reserved[100], 0xCD);
        assert_eq!(parsed.reserved[204], 0xEF);
    }

    #[test]
    fn pk2_default_key_creates_valid_header() {
        let bf = Blowfish::new(b"169841").unwrap();
        let header = PackHeader::new_encrypted(&bf);

        assert!(header.validate_sig().is_ok());
        assert!(header.verify(&bf).is_ok());
    }
}
