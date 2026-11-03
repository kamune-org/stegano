//! Key derivation and authenticated encryption.
//!
//! One Argon2id derivation per embed/extract produces a master key, which HKDF
//! splits into independent subkeys for the embedding permutation, the header and
//! the body. Every embed draws a fresh random salt, so each master key is used
//! for exactly one message and the AEAD nonces can be fixed.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};

pub(crate) const SALT_LEN: usize = 16;
pub(crate) const TAG_LEN: usize = 16;

// Argon2id, 64 MiB, 3 passes. Comfortably above OWASP's minimum and still fast
// enough on phones. Changing these requires a new format version.
const ARGON2_M_KIB: u32 = 64 * 1024;
const ARGON2_T: u32 = 3;
const ARGON2_P: u32 = 1;

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct Keys {
    pub permutation: [u8; 32],
    header: [u8; 32],
    body: [u8; 32],
}

impl Keys {
    pub fn derive(passphrase: &str, salt: &[u8; SALT_LEN]) -> Result<Self> {
        let params = Params::new(ARGON2_M_KIB, ARGON2_T, ARGON2_P, Some(32))
            .map_err(|e| Error::Internal(e.to_string()))?;
        let mut master = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase.as_bytes(), salt, &mut master)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let hk = Hkdf::<Sha256>::new(Some(b"stegano/v2"), &master);
        master.zeroize();
        let mut keys = Keys {
            permutation: [0; 32],
            header: [0; 32],
            body: [0; 32],
        };
        for (info, out) in [
            (&b"permutation"[..], &mut keys.permutation),
            (b"header", &mut keys.header),
            (b"body", &mut keys.body),
        ] {
            hk.expand(info, out)
                .map_err(|e| Error::Internal(e.to_string()))?;
        }
        Ok(keys)
    }

    pub fn seal_header(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        seal(&self.header, b"stegano/v2/header", plaintext)
    }

    pub fn open_header(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        open(&self.header, b"stegano/v2/header", ciphertext)
    }

    pub fn seal_body(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        seal(&self.body, b"stegano/v2/body", plaintext)
    }

    pub fn open_body(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        open(&self.body, b"stegano/v2/body", ciphertext)
    }
}

fn seal(key: &[u8; 32], aad: &[u8], msg: &[u8]) -> Result<Vec<u8>> {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(&Nonce::default(), Payload { msg, aad })
        .map_err(|_| Error::Internal("encryption failed".into()))
}

fn open(key: &[u8; 32], aad: &[u8], msg: &[u8]) -> Result<Vec<u8>> {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(&Nonce::default(), Payload { msg, aad })
        .map_err(|_| Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subkeys_differ_and_roundtrip() {
        let keys = Keys::derive("correct horse", &[7; SALT_LEN]).unwrap();
        assert_ne!(keys.permutation, keys.header);
        assert_ne!(keys.header, keys.body);

        let ct = keys.seal_body(b"hello").unwrap();
        assert_eq!(ct.len(), 5 + TAG_LEN);
        assert_eq!(keys.open_body(&ct).unwrap(), b"hello");
        // Header and body keys are not interchangeable.
        assert!(matches!(keys.open_header(&ct), Err(Error::NotFound)));

        let other = Keys::derive("wrong horse", &[7; SALT_LEN]).unwrap();
        assert!(matches!(other.open_body(&ct), Err(Error::NotFound)));
    }
}
