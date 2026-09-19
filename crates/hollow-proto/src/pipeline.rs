//! The whole decode chain as one deterministic object:
//! UDP payload -> KCP reassembly -> envelope -> decrypt -> un-XOR -> decode.

use serde::{Deserialize, Serialize};

use crate::crypto::session::Session;
pub use crate::crypto::session::SessionState;
use crate::decode::{self, Decoded};
use crate::envelope;
use crate::frame;
use crate::kcp;
use crate::model::{Agent, DriveDisc, WEngine};
use crate::proto::datamine::Datamine;
use crate::proto::schema::Schema;
use crate::proto::wire;
use crate::GAME_PORT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Server -> client.
    Incoming,
    /// Client -> server.
    Outgoing,
}

impl Direction {
    pub fn from_ports(src_port: u16, dst_port: u16) -> Option<Self> {
        if dst_port == GAME_PORT {
            Some(Direction::Outgoing)
        } else if src_port == GAME_PORT {
            Some(Direction::Incoming)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ServerKeyFound,
    SessionEstablished {
        /// Seconds between the packet timestamp and the client clock seed that worked.
        clock_delta: i64,
    },
    Agents(Vec<Agent>),
    WEngines(Vec<WEngine>),
    Discs(Vec<DriveDisc>),
    UnhandledCommand {
        cmd_id: u16,
        len: usize,
    },
    Warning(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub datagrams: u64,
    pub messages: u64,
    pub undecodable: u64,
    pub kcp_gaps: u32,
}

pub struct Pipeline {
    incoming: kcp::Stream,
    outgoing: kcp::Stream,
    session: Session,
    dm: Datamine,
    schema: Schema,
    stats: Stats,
    brute_force_failures: u32,
}

/// Minimum body size worth brute-forcing against (a tiny body can parse under a wrong key by chance).
const MIN_BRUTE_FORCE_BODY: usize = 32;
/// Give up warning after this many failed brute-force attempts (probably the wrong region).
const MAX_BRUTE_FORCE_WARNINGS: u32 = 3;

impl Pipeline {
    pub fn new(initial_seed: u64, dm: Datamine, schema: Schema) -> Self {
        Self {
            incoming: kcp::Stream::default(),
            outgoing: kcp::Stream::default(),
            session: Session::new(initial_seed),
            dm,
            schema,
            stats: Stats::default(),
            brute_force_failures: 0,
        }
    }

    /// Pipeline for a named region using the vendored data files.
    pub fn for_region(region: &str) -> crate::error::Result<Self> {
        let dm = Datamine::vendored();
        let seed = dm.region_seed(region)?;
        let schema = Schema::from_json(crate::vendored::NAP)?;
        Ok(Self::new(seed, dm, schema))
    }

    pub fn state(&self) -> SessionState {
        self.session.state()
    }

    pub fn stats(&self) -> Stats {
        Stats {
            kcp_gaps: self.incoming.gaps + self.outgoing.gaps,
            ..self.stats
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Feed a raw Ethernet frame; non-game traffic is ignored.
    pub fn feed_frame(&mut self, frame: &[u8], unix_secs: i64) -> Vec<Event> {
        let Some(udp) = frame::ethernet(frame) else {
            return Vec::new();
        };
        let Some(dir) = Direction::from_ports(udp.src_port, udp.dst_port) else {
            return Vec::new();
        };
        self.feed(udp.payload, dir, unix_secs)
    }

    /// Feed one UDP payload captured at `unix_secs`.
    pub fn feed(&mut self, payload: &[u8], dir: Direction, unix_secs: i64) -> Vec<Event> {
        self.stats.datagrams += 1;
        let stream = match dir {
            Direction::Incoming => &mut self.incoming,
            Direction::Outgoing => &mut self.outgoing,
        };
        let messages = stream.feed(payload);
        let mut events = Vec::new();
        for m in messages {
            self.stats.messages += 1;
            self.handle_message(&m, unix_secs, &mut events);
        }
        events
    }

    fn handle_message(&mut self, msg: &[u8], unix_secs: i64, events: &mut Vec<Event>) {
        let Some(env) = envelope::parse(msg) else {
            self.stats.undecodable += 1;
            return;
        };
        let cmd_id = env.header.cmd_id;
        let body = env.body;

        match self.session.state() {
            SessionState::Initial => {
                if cmd_id == self.dm.cmd_player_get_token_sc_rsp {
                    let dec = self.session.decrypt(body);
                    match self.session.extract_server_rand_key(&dec) {
                        Ok(_) => events.push(Event::ServerKeyFound),
                        Err(e) => events.push(Event::Warning(format!("PlayerGetTokenScRsp seen but {e}"))),
                    }
                    return;
                }
            }
            SessionState::HaveServerKey => {
                if body.len() >= MIN_BRUTE_FORCE_BODY {
                    match self.session.derive_session_key(body, unix_secs) {
                        Some(_) => events.push(Event::SessionEstablished {
                            clock_delta: self.session.found_delta.unwrap_or(0),
                        }),
                        None => {
                            self.brute_force_failures += 1;
                            if self.brute_force_failures <= MAX_BRUTE_FORCE_WARNINGS {
                                events.push(Event::Warning(format!(
                                    "could not derive session key from cmd {cmd_id} ({} bytes) at t={unix_secs}",
                                    body.len()
                                )));
                            }
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
            SessionState::Established => {}
        }

        if body.is_empty() {
            return;
        }
        let dec = self.session.decrypt(body);
        let mut fields = match wire::parse(&dec) {
            Ok(f) => f,
            Err(e) => {
                self.stats.undecodable += 1;
                if self.session.state() == SessionState::Established {
                    events.push(Event::Warning(format!("cmd {cmd_id}: {e}")));
                }
                return;
            }
        };
        self.schema.unxor_cmd(cmd_id, &mut fields);
        match decode::decode(&self.dm, cmd_id, &fields) {
            Some(Decoded::Agents(v)) => events.push(Event::Agents(v)),
            Some(Decoded::WEngines(v)) => events.push(Event::WEngines(v)),
            Some(Decoded::Discs(v)) => events.push(Event::Discs(v)),
            None => events.push(Event::UnhandledCommand { cmd_id, len: dec.len() }),
        }
    }
}

/// Debug variant of [`Pipeline`] that hands back decrypted, un-XORed top-level
/// fields for every message instead of decoding them. Used by examples/tools.
pub struct RawPipeline {
    incoming: kcp::Stream,
    outgoing: kcp::Stream,
    session: Session,
    dm: Datamine,
    schema: Schema,
}

impl RawPipeline {
    pub fn new(initial_seed: u64, dm: Datamine, schema: Schema) -> Self {
        Self {
            incoming: kcp::Stream::default(),
            outgoing: kcp::Stream::default(),
            session: Session::new(initial_seed),
            dm,
            schema,
        }
    }

    pub fn feed_raw(&mut self, payload: &[u8], dir: Direction, unix_secs: i64) -> Vec<(u16, Vec<wire::Field>)> {
        let stream = match dir {
            Direction::Incoming => &mut self.incoming,
            Direction::Outgoing => &mut self.outgoing,
        };
        let mut out = Vec::new();
        for m in stream.feed(payload) {
            let Some(env) = envelope::parse(&m) else { continue };
            let cmd_id = env.header.cmd_id;
            let body = env.body;
            match self.session.state() {
                SessionState::Initial => {
                    if cmd_id == self.dm.cmd_player_get_token_sc_rsp {
                        let dec = self.session.decrypt(body);
                        let _ = self.session.extract_server_rand_key(&dec);
                    }
                    continue;
                }
                SessionState::HaveServerKey => {
                    if body.len() < MIN_BRUTE_FORCE_BODY || self.session.derive_session_key(body, unix_secs).is_none() {
                        continue;
                    }
                }
                SessionState::Established => {}
            }
            if let Ok(mut fields) = wire::parse(&self.session.decrypt(body)) {
                self.schema.unxor_cmd(cmd_id, &mut fields);
                out.push((cmd_id, fields));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::cs_random::{client_rand_key, seed_from_unix_secs};
    use crate::crypto::xorpad;
    use crate::proto::wire::{bytes, encode, message, varint};
    use base64::prelude::*;

    const SEED: u64 = 0x1234_5678_9ABC_DEF0;
    const SERVER_KEY: u64 = 0xFEED_FACE_CAFE_BEEF;

    struct Rng(u64);
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

    /// One KCP PUSH datagram wrapping one game message.
    fn datagram(sn: u32, cmd_id: u16, body: &[u8]) -> Vec<u8> {
        let mut msg = Vec::new();
        envelope::write(cmd_id, &[], body, &mut msg);
        let mut d = Vec::new();
        kcp::Header {
            conv: 1,
            token: 1,
            cmd: kcp::CMD_PUSH,
            frg: 0,
            wnd: 32,
            ts: 0,
            sn,
            una: 0,
            len: msg.len() as u32,
        }
        .write(&mut d);
        d.extend(msg);
        d
    }

    #[test]
    fn synthetic_login() {
        let dm = Datamine::vendored();
        let mut p = Pipeline::new(SEED, Datamine::vendored(), Schema::from_entries(vec![]));
        let t = 1_726_600_000i64;

        // 1. token response under the initial pad
        let ct = crate::crypto::rsa::public_key()
            .encrypt(&mut Rng(99), ::rsa::Pkcs1v15Encrypt, &SERVER_KEY.to_le_bytes())
            .unwrap();
        let tok = encode(&[bytes(5, BASE64_STANDARD.encode(ct).into_bytes())]);
        let tok_enc = xorpad::xored(&xorpad::initial(SEED), &tok);
        assert_eq!(
            p.feed(
                &datagram(0, dm.cmd_player_get_token_sc_rsp, &tok_enc),
                Direction::Incoming,
                t
            ),
            vec![Event::ServerKeyFound]
        );
        assert_eq!(p.state(), SessionState::HaveServerKey);

        // 2. next message (outgoing, as in real traffic) under the session pad
        let key = SERVER_KEY ^ client_rand_key(seed_from_unix_secs(t));
        let spad = xorpad::session(key);
        let req = encode(&[varint(1, 1), bytes(2, vec![7u8; 48])]);
        let ev = p.feed(
            &datagram(0, 100, &xorpad::xored(&spad, &req)),
            Direction::Outgoing,
            t + 1,
        );
        assert_eq!(
            ev,
            vec![
                Event::SessionEstablished { clock_delta: -1 },
                Event::UnhandledCommand {
                    cmd_id: 100,
                    len: req.len()
                }
            ]
        );

        // 3. weapon data
        let w = &dm.weapon_info;
        let wd = encode(&[message(
            dm.weapon_data.weapons,
            &[varint(w.id, 12001), varint(w.uid, 3), varint(w.level, 10)],
        )]);
        let ev = p.feed(
            &datagram(1, dm.cmd_get_weapon_data_sc_rsp, &xorpad::xored(&spad, &wd)),
            Direction::Incoming,
            t + 2,
        );
        assert_eq!(
            ev,
            vec![Event::WEngines(vec![WEngine {
                id: 12001,
                uid: 3,
                level: 10,
                phase: 0,
                modification: 0,
                lock: false
            }])]
        );

        // 4. garbage after establishment is reported, state sticks
        let ev = p.feed(&datagram(2, 7, &[0xff; 16]), Direction::Incoming, t + 3);
        assert!(matches!(ev.as_slice(), [Event::Warning(_)]));
        assert_eq!(p.state(), SessionState::Established);
        assert_eq!(p.stats().messages, 4);
    }

    #[test]
    fn wrong_clock_never_establishes() {
        let dm = Datamine::vendored();
        let mut p = Pipeline::new(SEED, Datamine::vendored(), Schema::from_entries(vec![]));
        let t = 1_726_600_000i64;
        let ct = crate::crypto::rsa::public_key()
            .encrypt(&mut Rng(5), ::rsa::Pkcs1v15Encrypt, &SERVER_KEY.to_le_bytes())
            .unwrap();
        let tok = encode(&[bytes(5, BASE64_STANDARD.encode(ct).into_bytes())]);
        p.feed(
            &datagram(
                0,
                dm.cmd_player_get_token_sc_rsp,
                &xorpad::xored(&xorpad::initial(SEED), &tok),
            ),
            Direction::Incoming,
            t,
        );
        let key = SERVER_KEY ^ client_rand_key(seed_from_unix_secs(t));
        let body = xorpad::xored(&xorpad::session(key), &encode(&[bytes(1, vec![1u8; 64])]));
        for i in 0..5 {
            let ev = p.feed(&datagram(i, 100, &body), Direction::Outgoing, t + 60);
            assert!(ev.iter().all(|e| matches!(e, Event::Warning(_))), "{ev:?}");
        }
        assert_eq!(p.state(), SessionState::HaveServerKey);
        // and a nudge back inside the window still works
        let ev = p.feed(&datagram(5, 100, &body), Direction::Outgoing, t + 4);
        assert!(matches!(ev.first(), Some(Event::SessionEstablished { .. })));
    }

    #[test]
    fn frames_filter_ports() {
        let mut p = Pipeline::for_region("America").unwrap();
        let mut f = vec![0u8; 12];
        f.extend_from_slice(&[
            0x08, 0x00, 0x45, 0, 0, 30, 0, 0, 0, 0, 64, 17, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
        ]);
        f.extend_from_slice(&[0x00, 0x35, 0x00, 0x35, 0, 10, 0, 0, 1, 2]);
        assert!(p.feed_frame(&f, 0).is_empty());
        assert_eq!(p.stats().datagrams, 0);
    }
}
