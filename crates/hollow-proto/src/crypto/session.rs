//! Session-key state machine: initial pad -> server key from the token response
//! -> brute-forced client key -> session pad.

use base64::prelude::*;

use super::constants::{BRUTE_FORCE_WINDOW_SECS, RSA_KEY_SIZE};
use super::cs_random::{client_rand_key, seed_from_unix_secs};
use super::rsa;
use super::xorpad::{self, Pad};
use crate::proto::wire;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Only the region's initial pad is known.
    Initial,
    /// `server_rand_key` extracted; waiting for a body large enough to brute force against.
    HaveServerKey,
    /// Session pad derived; all further bodies decrypt with it.
    Established,
}

pub struct Session {
    initial_pad: Box<Pad>,
    session_pad: Option<Box<Pad>>,
    pub server_rand_key: Option<u64>,
    pub client_rand_key: Option<u64>,
    pub session_key: Option<u64>,
    /// Timestamp-delta (seconds) at which the brute force succeeded, for diagnostics.
    pub found_delta: Option<i64>,
}

impl Session {
    pub fn new(initial_seed: u64) -> Self {
        Self {
            initial_pad: xorpad::initial(initial_seed),
            session_pad: None,
            server_rand_key: None,
            client_rand_key: None,
            session_key: None,
            found_delta: None,
        }
    }

    pub fn state(&self) -> SessionState {
        match (self.server_rand_key, &self.session_pad) {
            (_, Some(_)) => SessionState::Established,
            (Some(_), None) => SessionState::HaveServerKey,
            (None, None) => SessionState::Initial,
        }
    }

    fn pad(&self) -> &Pad {
        self.session_pad.as_deref().unwrap_or(&self.initial_pad)
    }

    /// Decrypt a body with whichever pad is current.
    pub fn decrypt(&self, body: &[u8]) -> Vec<u8> {
        xorpad::xored(self.pad(), body)
    }

    /// Pull `server_rand_key` out of a decrypted `PlayerGetTokenScRsp` body: the
    /// length-delimited field that base64-decodes to one RSA block.
    pub fn extract_server_rand_key(&mut self, decrypted: &[u8]) -> Result<u64, String> {
        let fields = wire::parse(decrypted).map_err(|e| format!("token response is not protobuf: {e}"))?;
        let mut last_err = String::from("no RSA-sized field in token response");
        for f in fields.iter().filter_map(|f| f.bytes()) {
            let Ok(raw) = BASE64_STANDARD.decode(f) else {
                continue;
            };
            if raw.len() != RSA_KEY_SIZE {
                continue;
            }
            match rsa::decrypt_block(&raw) {
                Ok(plain) if plain.len() == 8 => {
                    let key = u64::from_le_bytes(plain.try_into().unwrap());
                    self.server_rand_key = Some(key);
                    return Ok(key);
                }
                Ok(plain) => last_err = format!("decrypted key has {} bytes, expected 8", plain.len()),
                Err(e) => last_err = e.to_string(),
            }
        }
        Err(last_err)
    }

    /// Try every clock offset in the window; the first pad under which the body
    /// parses as protobuf is accepted.
    pub fn derive_session_key(&mut self, encrypted_body: &[u8], unix_secs: i64) -> Option<u64> {
        let server = self.server_rand_key?;
        for delta in -BRUTE_FORCE_WINDOW_SECS..=BRUTE_FORCE_WINDOW_SECS {
            let seed = seed_from_unix_secs(unix_secs + delta);
            let crk = client_rand_key(seed);
            let key = server ^ crk;
            let pad = xorpad::session(key);
            if wire::parse(&xorpad::xored(&pad, encrypted_body)).is_ok() {
                self.client_rand_key = Some(crk);
                self.session_key = Some(key);
                self.session_pad = Some(pad);
                self.found_delta = Some(delta);
                return Some(key);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::wire::{bytes, encode, varint};
    use rsa_test_rng::Rng;

    mod rsa_test_rng {
        pub struct Rng(u64);
        impl Rng {
            pub fn new() -> Self {
                Rng(12345)
            }
        }
        impl ::rsa::rand_core::RngCore for Rng {
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
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), ::rsa::rand_core::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }
        impl ::rsa::rand_core::CryptoRng for Rng {}
    }

    const SEED: u64 = 0x50C2_1982_AC00_9AF2;
    const SERVER_KEY: u64 = 0x0123_4567_89AB_CDEF;

    fn token_response() -> Vec<u8> {
        let ct = rsa::public_key()
            .encrypt(&mut Rng::new(), ::rsa::Pkcs1v15Encrypt, &SERVER_KEY.to_le_bytes())
            .unwrap();
        let msg = encode(&[
            varint(1, 0),
            bytes(2, b"decoy".to_vec()),
            bytes(3, BASE64_STANDARD.encode(ct).into_bytes()),
            bytes(4, vec![0u8; RSA_KEY_SIZE]), // right size, not base64 of a block
        ]);
        xorpad::xored(&xorpad::initial(SEED), &msg)
    }

    #[test]
    fn full_handshake() {
        let mut s = Session::new(SEED);
        assert_eq!(s.state(), SessionState::Initial);

        let dec = s.decrypt(&token_response());
        assert_eq!(s.extract_server_rand_key(&dec).unwrap(), SERVER_KEY);
        assert_eq!(s.state(), SessionState::HaveServerKey);

        // Client seeds from its clock at t; we observe at t+3.
        let t = 1_726_600_000i64;
        let crk = client_rand_key(seed_from_unix_secs(t));
        let key = SERVER_KEY ^ crk;
        let plain = encode(&[varint(1, 7), bytes(2, vec![1u8; 40]), varint(3, 99)]);
        let enc = xorpad::xored(&xorpad::session(key), &plain);

        assert_eq!(s.derive_session_key(&enc, t + 9), None);
        assert_eq!(s.state(), SessionState::HaveServerKey);

        assert_eq!(s.derive_session_key(&enc, t + 3), Some(key));
        assert_eq!(s.state(), SessionState::Established);
        assert_eq!(s.found_delta, Some(-3));
        assert_eq!(s.decrypt(&enc), plain);
    }

    #[test]
    fn extract_fails_cleanly_on_garbage() {
        let mut s = Session::new(SEED);
        assert!(s.extract_server_rand_key(&[0xff, 0xff, 0xff]).is_err());
        assert!(s.extract_server_rand_key(&encode(&[varint(1, 1)])).is_err());
        assert_eq!(s.state(), SessionState::Initial);
        assert_eq!(s.derive_session_key(&[0; 64], 0), None);
    }
}
