//! Decoder for Zenless Zone Zero login traffic, ported from
//! [zzz_packet_capture](https://github.com/AleXu224/zzz_packet_capture) (MIT).
//!
//! The crate is pure: feed it UDP payloads (or whole frames) with capture
//! timestamps and it yields decoded Agents / W-Engines / Drive Discs. No
//! networking, no capture backend. See `specs/implementation.md`.

pub mod crypto;
pub mod decode;
pub mod envelope;
pub mod error;
pub mod export;
pub mod fixture;
pub mod frame;
pub mod gamedata;
pub mod kcp;
pub mod model;
pub mod pcapng;
pub mod pipeline;
pub mod proto;

pub use error::ProtoError;
pub use pipeline::{Direction, Event, Pipeline, SessionState};

/// Game server UDP port. Fixed for all regions as of 3.2.
pub const GAME_PORT: u16 = 20501;

/// Vendored data files (fallbacks when nothing newer is cached).
pub mod vendored {
    pub const DATAMINE: &str = include_str!("../data/datamine.json");
    pub const NAP: &str = include_str!("../data/nap.json");
    pub const MANIFEST: &str = include_str!("../data/manifest.json");
    pub const NANOKA: &str = include_str!("../data/nanoka.json");
}
