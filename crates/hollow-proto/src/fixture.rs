//! Recorded captures: pcapng (primary; what `pktmon pcapng` and Wireshark
//! produce) and zzz_packet_capture's `captured_packets.json` (secondary).
//! Both reduce to a list of `(timestamp, direction, UDP payload)`.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{ProtoError, Result};
use crate::frame;
use crate::pcapng;
use crate::pipeline::{Direction, Event, Pipeline};
use crate::GAME_PORT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// Capture time since the Unix epoch.
    pub timestamp: Duration,
    pub direction: Direction,
    /// UDP payload only.
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn unix_secs(&self) -> i64 {
        self.timestamp.as_secs() as i64
    }
}

/// Load by extension: `.pcapng` or `.json`.
pub fn read(path: &Path) -> Result<Vec<Packet>> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pcapng") => read_pcapng(path),
        Some("json") => read_json(path),
        other => Err(ProtoError::Fixture(format!("unsupported fixture extension {other:?}"))),
    }
}

/// Read a pcapng, keeping only UDP datagrams to/from the game port.
pub fn read_pcapng(path: &Path) -> Result<Vec<Packet>> {
    let bytes = std::fs::read(path)?;
    let mut out = Vec::new();
    for p in pcapng::packets(&bytes)? {
        let udp = match p.interface.linktype {
            pcapng::LINKTYPE_ETHERNET => frame::ethernet(p.data),
            pcapng::LINKTYPE_RAW | pcapng::LINKTYPE_IPV4 | pcapng::LINKTYPE_IPV6 => frame::ip(p.data),
            _ => None,
        };
        let Some(udp) = udp else { continue };
        let Some(direction) = Direction::from_ports(udp.src_port, udp.dst_port) else {
            continue;
        };
        out.push(Packet {
            timestamp: p.timestamp,
            direction,
            payload: udp.payload.to_vec(),
        });
    }
    Ok(out)
}

/// Write packets as a pcapng with synthesised Ethernet/IPv4/UDP headers
/// (addresses are placeholders; only ports and payloads matter).
pub fn write_pcapng(path: &Path, packets: &[Packet]) -> Result<()> {
    let bytes = pcapng::write(packets.iter().map(|p| (p.timestamp, synth_frame(p))));
    BufWriter::new(File::create(path)?).write_all(&bytes)?;
    Ok(())
}

fn synth_frame(p: &Packet) -> Vec<u8> {
    let (src_port, dst_port) = match p.direction {
        Direction::Outgoing => (50000u16, GAME_PORT),
        Direction::Incoming => (GAME_PORT, 50000u16),
    };
    let udp_len = 8 + p.payload.len();
    let total = 20 + udp_len;
    let mut f = Vec::with_capacity(14 + total);
    f.extend_from_slice(&[0; 12]);
    f.extend_from_slice(&[0x08, 0x00]);
    f.extend_from_slice(&[0x45, 0, (total >> 8) as u8, total as u8, 0, 0, 0x40, 0, 64, 17, 0, 0]);
    f.extend_from_slice(&[10, 0, 0, 1, 10, 0, 0, 2]);
    f.extend_from_slice(&src_port.to_be_bytes());
    f.extend_from_slice(&dst_port.to_be_bytes());
    f.extend_from_slice(&(udp_len as u16).to_be_bytes());
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(&p.payload);
    f
}

// --- zzz_packet_capture `captured_packets.json` -------------------------------

#[derive(Serialize, Deserialize)]
struct JsonPacketList {
    packets: Vec<JsonPacket>,
}

#[derive(Serialize, Deserialize)]
struct JsonPacket {
    direction: JsonDirection,
    timestamp: i64,
    data: Vec<u8>,
}

/// The reference serialises its enum as an integer (incoming = 0, outgoing = 1)
/// but accept names too.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum JsonDirection {
    Index(u8),
    Name(String),
}

impl TryFrom<&JsonDirection> for Direction {
    type Error = ProtoError;
    fn try_from(d: &JsonDirection) -> Result<Self> {
        match d {
            JsonDirection::Index(0) => Ok(Direction::Incoming),
            JsonDirection::Index(1) => Ok(Direction::Outgoing),
            JsonDirection::Name(n) if n.eq_ignore_ascii_case("incoming") => Ok(Direction::Incoming),
            JsonDirection::Name(n) if n.eq_ignore_ascii_case("outgoing") => Ok(Direction::Outgoing),
            _ => Err(ProtoError::Fixture("bad direction".into())),
        }
    }
}

pub fn read_json(path: &Path) -> Result<Vec<Packet>> {
    let list: JsonPacketList = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    list.packets
        .iter()
        .map(|p| {
            Ok(Packet {
                timestamp: Duration::from_secs(u64::try_from(p.timestamp).unwrap_or(0)),
                direction: Direction::try_from(&p.direction)?,
                payload: p.data.clone(),
            })
        })
        .collect()
}

pub fn write_json(path: &Path, packets: &[Packet]) -> Result<()> {
    let list = JsonPacketList {
        packets: packets
            .iter()
            .map(|p| JsonPacket {
                direction: JsonDirection::Index(match p.direction {
                    Direction::Incoming => 0,
                    Direction::Outgoing => 1,
                }),
                timestamp: p.unix_secs(),
                data: p.payload.clone(),
            })
            .collect(),
    };
    serde_json::to_writer(BufWriter::new(File::create(path)?), &list)?;
    Ok(())
}

/// Run a recording through a pipeline, returning every event in order.
pub fn replay(pipeline: &mut Pipeline, packets: &[Packet]) -> Vec<Event> {
    packets
        .iter()
        .flat_map(|p| pipeline.feed(&p.payload, p.direction, p.unix_secs()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Packet> {
        vec![
            Packet {
                timestamp: Duration::new(1_726_600_000, 123_456_000),
                direction: Direction::Outgoing,
                payload: vec![1, 2, 3],
            },
            Packet {
                timestamp: Duration::new(1_726_600_001, 0),
                direction: Direction::Incoming,
                payload: vec![9; 1400],
            },
        ]
    }

    #[test]
    fn pcapng_roundtrip() {
        let dir = std::env::temp_dir().join("hollow-proto-fixture-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rt.pcapng");
        write_pcapng(&path, &sample()).unwrap();
        assert_eq!(read(&path).unwrap(), sample());
    }

    #[test]
    fn json_roundtrip_and_interop() {
        let dir = std::env::temp_dir().join("hollow-proto-fixture-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rt.json");
        write_json(&path, &sample()).unwrap();
        let back = read(&path).unwrap();
        // JSON only keeps whole seconds
        assert_eq!(back[0].unix_secs(), 1_726_600_000);
        assert_eq!(back[1].payload.len(), 1400);
        assert_eq!(back[0].direction, Direction::Outgoing);

        let named = dir.join("named.json");
        std::fs::write(
            &named,
            r#"{"packets":[{"direction":"incoming","timestamp":5,"data":[1]}]}"#,
        )
        .unwrap();
        assert_eq!(read(&named).unwrap()[0].direction, Direction::Incoming);
        assert!(read(&dir.join("x.pcap")).is_err());
    }
}
