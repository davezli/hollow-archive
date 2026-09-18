//! RSA-1024 PKCS#1 v1.5 decryption of `server_rand_key` with the embedded client key.

use std::sync::OnceLock;

use base64::prelude::*;
use rsa::{BigUint, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};

use super::constants::*;
use crate::error::{ProtoError, Result};

fn b64(s: &str) -> BigUint {
    BigUint::from_bytes_be(&BASE64_STANDARD.decode(s).expect("valid base64 constant"))
}

pub fn private_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    KEY.get_or_init(|| {
        RsaPrivateKey::from_components(
            b64(RSA_MODULUS_B64),
            b64(RSA_EXPONENT_B64),
            b64(RSA_D_B64),
            vec![b64(RSA_P_B64), b64(RSA_Q_B64)],
        )
        .expect("embedded RSA key is consistent")
    })
}

pub fn public_key() -> RsaPublicKey {
    RsaPublicKey::from(private_key())
}

/// Decrypt one 128-byte block.
pub fn decrypt_block(ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() != RSA_KEY_SIZE {
        return Err(ProtoError::Rsa(format!(
            "block must be {RSA_KEY_SIZE} bytes, got {}",
            ciphertext.len()
        )));
    }
    private_key()
        .decrypt(Pkcs1v15Encrypt, ciphertext)
        .map_err(|e| ProtoError::Rsa(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core_shim::Rng;

    // `rsa` needs an RNG for encryption padding; a tiny deterministic one is enough for tests.
    mod rand_core_shim {
        pub struct Rng(u64);
        impl Rng {
            pub fn new() -> Self {
                Rng(0x9E37_79B9_7F4A_7C15)
            }
        }
        impl rsa::rand_core::RngCore for Rng {
            fn next_u32(&mut self) -> u32 {
                self.next_u64() as u32
            }
            fn next_u64(&mut self) -> u64 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                self.0
            }
            fn fill_bytes(&mut self, dest: &mut [u8]) {
                for b in dest {
                    *b = self.next_u64() as u8;
                }
            }
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), rsa::rand_core::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }
        impl rsa::rand_core::CryptoRng for Rng {}
    }

    #[test]
    fn roundtrip_with_public_half() {
        let plain = 0x1122_3344_5566_7788u64.to_le_bytes();
        let ct = public_key().encrypt(&mut Rng::new(), Pkcs1v15Encrypt, &plain).unwrap();
        assert_eq!(ct.len(), RSA_KEY_SIZE);
        assert_eq!(decrypt_block(&ct).unwrap(), plain);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(decrypt_block(&[0u8; 64]).is_err());
        assert!(decrypt_block(&[0xffu8; RSA_KEY_SIZE]).is_err());
    }
}
