use block_modes::BlockMode;

use std::cell::UnsafeCell;

use crate::constants::PK2_SALT;
use crate::error::{Error, Pk2Result};

type BlowfishImpl = block_modes::Ecb<blowfish::BlowfishLE, block_modes::block_padding::ZeroPadding>;

// Wrapper around the blowfish crates implementation cause it requires
// mutability without mutating state. This simplifies our implementation A LOT.
pub struct Blowfish {
    inner: UnsafeCell<BlowfishImpl>,
}

impl Blowfish {
    pub fn new(key: &[u8]) -> Pk2Result<Self> {
        let mut key = key.to_vec();
        gen_final_blowfish_key_inplace(&mut key);
        match BlowfishImpl::new_varkey(&key) {
            Ok(inner) => Ok(Blowfish {
                inner: UnsafeCell::new(inner),
            }),
            Err(_) => Err(Error::InvalidKey),
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

fn gen_final_blowfish_key_inplace(key: &mut [u8]) {
    let key_len = key.len().min(56);

    let mut base_key = [0; 56];
    base_key[0..PK2_SALT.len()].copy_from_slice(&PK2_SALT);

    for i in 0..key_len {
        key[i] ^= base_key[i];
    }
}
