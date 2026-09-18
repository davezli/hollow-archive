//! Minimal pcapng reader/writer: Section Header, Interface Description and
//! Enhanced Packet blocks only. Everything else is skipped. Tolerant of
//! unknown options (pktmon writes some that stricter parsers reject).

use std::time::Duration;

use crate::error::{ProtoError, Result};

const SHB: u32 = 0x0A0D_0D0A;
const IDB: u32 = 0x0000_0001;
const EPB: u32 = 0x0000_0006;
const BYTE_ORDER_MAGIC: u32 = 0x1A2B_3C4D;
const OPT_IF_TSRESOL: u16 = 9;

pub const LINKTYPE_ETHERNET: u16 = 1;
pub const LINKTYPE_RAW: u16 = 101;
pub const LINKTYPE_IPV4: u16 = 228;
pub const LINKTYPE_IPV6: u16 = 229;

#[derive(Debug, Clone, Copy)]
pub struct Interface {
    pub linktype: u16,
    /// Timestamp units per second.
    pub ts_per_sec: u64,
}

#[derive(Debug, Clone)]
pub struct Packet<'a> {
    pub interface: Interface,
    pub timestamp: Duration,
    pub data: &'a [u8],
}

fn err(msg: &str) -> ProtoError {
    ProtoError::Fixture(format!("pcapng: {msg}"))
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
    big_endian: bool,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.buf.get(self.pos..self.pos + n).ok_or_else(|| err("truncated"))?;
        self.pos += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        let b: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(if self.big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }
    fn u32(&mut self) -> Result<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if self.big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }
}

fn parse_tsresol(options: &mut Cursor) -> Result<u64> {
    let mut ts_per_sec = 1_000_000;
    while let Ok(code) = options.u16() {
        let len = usize::from(options.u16()?);
        let value = options.take(len)?;
        options.pos += (4 - len % 4) % 4;
        if code == 0 {
            break;
        }
        if code == OPT_IF_TSRESOL {
            let v = *value.first().ok_or_else(|| err("empty if_tsresol"))?;
            ts_per_sec = if v & 0x80 == 0 {
                10u64.pow(u32::from(v))
            } else {
                1u64 << (v & 0x7f)
            };
        }
    }
    Ok(ts_per_sec)
}

/// Iterate packets in a pcapng file held in memory.
pub fn packets(file: &[u8]) -> Result<Vec<Packet<'_>>> {
    let mut out = Vec::new();
    let mut interfaces: Vec<Interface> = Vec::new();
    let mut big_endian = false;
    let mut pos = 0;
    while pos + 12 <= file.len() {
        // Block type is endian-dependent, but the SHB type is a palindrome.
        let raw_type = u32::from_le_bytes(file[pos..pos + 4].try_into().unwrap());
        if raw_type == SHB {
            let magic = u32::from_le_bytes(file[pos + 8..pos + 12].try_into().unwrap());
            big_endian = match magic {
                BYTE_ORDER_MAGIC => false,
                m if m == BYTE_ORDER_MAGIC.swap_bytes() => true,
                _ => return Err(err("bad byte-order magic")),
            };
            interfaces.clear();
        }
        let mut c = Cursor {
            buf: file,
            pos,
            big_endian,
        };
        let block_type = c.u32()?;
        let total_len = c.u32()? as usize;
        if total_len < 12 || pos + total_len > file.len() {
            return Err(err("bad block length"));
        }
        let body = &file[pos + 8..pos + total_len - 4];
        let mut b = Cursor {
            buf: body,
            pos: 0,
            big_endian,
        };
        match block_type {
            IDB => {
                let linktype = b.u16()?;
                b.u16()?; // reserved
                b.u32()?; // snaplen
                let ts_per_sec = parse_tsresol(&mut b)?;
                interfaces.push(Interface { linktype, ts_per_sec });
            }
            EPB => {
                let iface = b.u32()? as usize;
                let ts_hi = b.u32()?;
                let ts_lo = b.u32()?;
                let cap_len = b.u32()? as usize;
                b.u32()?; // original length
                let data = b.take(cap_len)?;
                let interface = *interfaces
                    .get(iface)
                    .ok_or_else(|| err("EPB references unknown interface"))?;
                let ticks = (u64::from(ts_hi) << 32) | u64::from(ts_lo);
                let secs = ticks / interface.ts_per_sec;
                let nanos = (ticks % interface.ts_per_sec) * 1_000_000_000 / interface.ts_per_sec;
                out.push(Packet {
                    interface,
                    timestamp: Duration::new(secs, nanos as u32),
                    data,
                });
            }
            _ => {}
        }
        pos += total_len;
    }
    Ok(out)
}

fn push_block(out: &mut Vec<u8>, block_type: u32, body: &[u8]) {
    let padded = body.len().div_ceil(4) * 4;
    let total = (12 + padded) as u32;
    out.extend_from_slice(&block_type.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(body);
    out.resize(out.len() + (padded - body.len()), 0);
    out.extend_from_slice(&total.to_le_bytes());
}

/// Serialise Ethernet frames with microsecond timestamps as a single-interface pcapng.
pub fn write(frames: impl IntoIterator<Item = (Duration, Vec<u8>)>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut shb = Vec::new();
    shb.extend_from_slice(&BYTE_ORDER_MAGIC.to_le_bytes());
    shb.extend_from_slice(&1u16.to_le_bytes());
    shb.extend_from_slice(&0u16.to_le_bytes());
    shb.extend_from_slice(&(-1i64).to_le_bytes());
    push_block(&mut out, SHB, &shb);

    let mut idb = Vec::new();
    idb.extend_from_slice(&LINKTYPE_ETHERNET.to_le_bytes());
    idb.extend_from_slice(&0u16.to_le_bytes());
    idb.extend_from_slice(&0u32.to_le_bytes());
    push_block(&mut out, IDB, &idb);

    for (ts, frame) in frames {
        let ticks = ts.as_micros() as u64;
        let mut epb = Vec::with_capacity(20 + frame.len());
        epb.extend_from_slice(&0u32.to_le_bytes());
        epb.extend_from_slice(&((ticks >> 32) as u32).to_le_bytes());
        epb.extend_from_slice(&(ticks as u32).to_le_bytes());
        epb.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        epb.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        epb.extend_from_slice(&frame);
        push_block(&mut out, EPB, &epb);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let frames = vec![
            (Duration::new(1_726_600_000, 123_456_000), vec![1u8, 2, 3]),
            (Duration::new(5, 0), vec![9u8; 7]),
        ];
        let bytes = write(frames.clone());
        let pk = packets(&bytes).unwrap();
        assert_eq!(pk.len(), 2);
        assert_eq!(pk[0].timestamp, frames[0].0);
        assert_eq!(pk[0].data, &frames[0].1[..]);
        assert_eq!(pk[1].data, &frames[1].1[..]);
        assert_eq!(pk[0].interface.linktype, LINKTYPE_ETHERNET);
    }

    #[test]
    fn tsresol_and_unknown_options() {
        // IDB with a custom option (code 2988) and if_tsresol = 9 (nanoseconds), big-endian section.
        let mut file = Vec::new();
        let mut shb = Vec::new();
        shb.extend_from_slice(&BYTE_ORDER_MAGIC.to_be_bytes());
        shb.extend_from_slice(&[0, 1, 0, 0]);
        shb.extend_from_slice(&(-1i64).to_be_bytes());
        let push = |out: &mut Vec<u8>, t: u32, body: &[u8]| {
            let padded = body.len().div_ceil(4) * 4;
            let total = (12 + padded) as u32;
            out.extend_from_slice(&t.to_be_bytes());
            out.extend_from_slice(&total.to_be_bytes());
            out.extend_from_slice(body);
            out.resize(out.len() + padded - body.len(), 0);
            out.extend_from_slice(&total.to_be_bytes());
        };
        push(&mut file, SHB, &shb);
        let mut idb = vec![0, 1, 0, 0, 0, 0, 0, 0];
        idb.extend_from_slice(&[0x0B, 0xAC, 0, 3, b'x', b'y', b'z', 0]); // unknown option, padded
        idb.extend_from_slice(&[0, 9, 0, 1, 9, 0, 0, 0]); // if_tsresol = 1e-9
        idb.extend_from_slice(&[0, 0, 0, 0]);
        push(&mut file, IDB, &idb);
        let ticks: u64 = 1_726_600_000 * 1_000_000_000 + 42;
        let mut epb = vec![0, 0, 0, 0];
        epb.extend_from_slice(&((ticks >> 32) as u32).to_be_bytes());
        epb.extend_from_slice(&(ticks as u32).to_be_bytes());
        epb.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 2, 0xAA, 0xBB]);
        push(&mut file, EPB, &epb);
        let pk = packets(&file).unwrap();
        assert_eq!(pk[0].timestamp, Duration::new(1_726_600_000, 42));
        assert_eq!(pk[0].data, &[0xAA, 0xBB]);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(packets(&[0x0A, 0x0D, 0x0D, 0x0A, 0xff, 0xff, 0xff, 0xff, 1, 2, 3, 4]).is_err());
        assert!(packets(&[1, 2, 3]).unwrap().is_empty());
    }
}
