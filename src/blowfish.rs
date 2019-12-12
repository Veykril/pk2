use block_modes::BlockMode;

use std::cell::UnsafeCell;

type BlowfishImpl = block_modes::Ecb<blowfish::BlowfishLE, block_modes::block_padding::ZeroPadding>;

// Wrapper around the blowfish crates implementation cause it requires
// mutability without mutating state. This simplifies our implementation A LOT.
pub struct Blowfish {
    inner: UnsafeCell<BlowfishImpl>,
}

impl Blowfish {
    pub fn new_varkey(key: &[u8]) -> crate::error::Pk2Result<Self> {
        match BlowfishImpl::new_varkey(key) {
            Ok(inner) => Ok(Blowfish {
                inner: UnsafeCell::new(inner),
            }),
            Err(_) => Err(crate::error::Error::InvalidKey),
        }
    }

    #[inline]
    pub fn decrypt(&self, buf: &mut [u8]) -> Result<(), block_modes::BlockModeError> {
        unsafe { &mut *self.inner.get() }.decrypt_nopad(buf)
    }
    #[inline]
    pub fn encrypt(&self, buf: &mut [u8]) -> Result<(), block_modes::BlockModeError> {
        unsafe { &mut *self.inner.get() }.encrypt_nopad(buf)
    }
}
