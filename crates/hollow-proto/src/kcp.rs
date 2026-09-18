//! mihoyo-flavoured KCP: 28-byte segment header (stock KCP + 4-byte `token`),
//! little-endian. Passive reassembly of PUSH segments into messages.

use std::collections::{BTreeMap, VecDeque};

pub const HEADER_SIZE: usize = 28;
pub const CMD_PUSH: u8 = 81;
pub const CMD_ACK: u8 = 82;
pub const CMD_WASK: u8 = 83;
pub const CMD_WINS: u8 = 84;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub conv: u32,
    pub token: u32,
    pub cmd: u8,
    pub frg: u8,
    pub wnd: u16,
    pub ts: u32,
    pub sn: u32,
    pub una: u32,
    pub len: u32,
}

impl Header {
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < HEADER_SIZE {
            return None;
        }
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Some(Header {
            conv: u32_at(0),
            token: u32_at(4),
            cmd: b[8],
            frg: b[9],
            wnd: u16::from_le_bytes([b[10], b[11]]),
            ts: u32_at(12),
            sn: u32_at(16),
            una: u32_at(20),
            len: u32_at(24),
        })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.conv.to_le_bytes());
        out.extend_from_slice(&self.token.to_le_bytes());
        out.push(self.cmd);
        out.push(self.frg);
        out.extend_from_slice(&self.wnd.to_le_bytes());
        out.extend_from_slice(&self.ts.to_le_bytes());
        out.extend_from_slice(&self.sn.to_le_bytes());
        out.extend_from_slice(&self.una.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
    }
}

#[derive(Debug, Clone)]
struct Segment {
    frg: u8,
    data: Vec<u8>,
}

/// One direction of a KCP conversation.
#[derive(Debug, Default)]
pub struct Stream {
    rcv_buf: BTreeMap<u32, Segment>,
    rcv_queue: VecDeque<Segment>,
    rcv_nxt: u32,
    rcv_nxt_known: bool,
    /// Number of times the buffer overflowed and we skipped ahead (lost data).
    pub gaps: u32,
}

impl Stream {
    /// Upper bound on out-of-order segments kept before we assume loss and skip ahead.
    pub const MAX_RCV_BUF: usize = 1024;

    /// Feed one UDP datagram (one or more KCP segments). Returns complete messages.
    pub fn feed(&mut self, mut data: &[u8]) -> Vec<Vec<u8>> {
        while let Some(h) = Header::parse(data) {
            let rest = &data[HEADER_SIZE..];
            let len = h.len as usize;
            if len > rest.len() {
                break; // truncated datagram: keep what we already queued
            }
            if h.cmd == CMD_PUSH && len > 0 {
                if !self.rcv_nxt_known {
                    self.rcv_nxt = h.sn;
                    self.rcv_nxt_known = true;
                }
                let diff = h.sn.wrapping_sub(self.rcv_nxt) as i32;
                if diff >= 0 && !self.rcv_buf.contains_key(&h.sn) {
                    self.rcv_buf.insert(
                        h.sn,
                        Segment {
                            frg: h.frg,
                            data: rest[..len].to_vec(),
                        },
                    );
                }
                self.promote();
                if self.rcv_buf.len() > Self::MAX_RCV_BUF {
                    // Passive observer: we cannot request a retransmit, so skip the hole.
                    self.rcv_nxt = *self.rcv_buf.keys().next().unwrap();
                    self.gaps += 1;
                    self.promote();
                }
            }
            data = &rest[len..];
        }
        self.drain()
    }

    fn promote(&mut self) {
        while let Some(seg) = self.rcv_buf.remove(&self.rcv_nxt) {
            self.rcv_queue.push_back(seg);
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
        }
    }

    fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(head) = self.rcv_queue.front() {
            let count = usize::from(head.frg) + 1;
            if self.rcv_queue.len() < count {
                break;
            }
            let complete = (0..count).all(|i| usize::from(self.rcv_queue[i].frg) == count - 1 - i);
            if !complete {
                self.rcv_queue.pop_front();
                continue;
            }
            let mut msg = Vec::new();
            for seg in self.rcv_queue.drain(..count) {
                msg.extend_from_slice(&seg.data);
            }
            out.push(msg);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(sn: u32, frg: u8, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        Header {
            conv: 1,
            token: 2,
            cmd: CMD_PUSH,
            frg,
            wnd: 32,
            ts: 0,
            sn,
            una: 0,
            len: data.len() as u32,
        }
        .write(&mut v);
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn header_roundtrip() {
        let h = Header {
            conv: 1,
            token: 0xdeadbeef,
            cmd: CMD_ACK,
            frg: 3,
            wnd: 7,
            ts: 9,
            sn: 11,
            una: 13,
            len: 0,
        };
        let mut v = Vec::new();
        h.write(&mut v);
        assert_eq!(v.len(), HEADER_SIZE);
        assert_eq!(Header::parse(&v), Some(h));
        assert_eq!(Header::parse(&v[..27]), None);
    }

    #[test]
    fn single_push() {
        let mut s = Stream::default();
        assert_eq!(s.feed(&seg(5, 0, b"hello")), vec![b"hello".to_vec()]);
    }

    #[test]
    fn fragments_in_order_same_datagram() {
        let mut s = Stream::default();
        let mut d = seg(0, 2, b"a");
        d.extend(seg(1, 1, b"b"));
        d.extend(seg(2, 0, b"c"));
        assert_eq!(s.feed(&d), vec![b"abc".to_vec()]);
    }

    #[test]
    fn fragments_out_of_order_across_datagrams() {
        let mut s = Stream::default();
        assert!(s.feed(&seg(10, 2, b"a")).is_empty());
        assert!(s.feed(&seg(12, 0, b"c")).is_empty());
        assert_eq!(s.feed(&seg(11, 1, b"b")), vec![b"abc".to_vec()]);
    }

    #[test]
    fn duplicates_and_control_segments_ignored() {
        let mut s = Stream::default();
        let mut ack = Vec::new();
        Header {
            conv: 1,
            token: 2,
            cmd: CMD_ACK,
            frg: 0,
            wnd: 32,
            ts: 0,
            sn: 99,
            una: 0,
            len: 0,
        }
        .write(&mut ack);
        assert!(s.feed(&ack).is_empty());
        assert_eq!(s.feed(&seg(0, 0, b"x")), vec![b"x".to_vec()]);
        assert!(s.feed(&seg(0, 0, b"x")).is_empty());
        assert!(s.feed(&seg(1, 0, b"")).is_empty());
    }

    #[test]
    fn truncated_datagram_keeps_earlier_segments() {
        let mut s = Stream::default();
        let mut d = seg(0, 0, b"ok");
        let mut bad = seg(1, 0, b"truncated");
        bad.truncate(bad.len() - 3);
        d.extend(bad);
        assert_eq!(s.feed(&d), vec![b"ok".to_vec()]);
    }

    #[test]
    fn loss_recovery_after_overflow() {
        let mut s = Stream::default();
        assert_eq!(s.feed(&seg(0, 0, b"first")), vec![b"first".to_vec()]);
        // sn 1 is lost; buffer MAX+1 later segments, the last one triggers the skip.
        let last = 2 + Stream::MAX_RCV_BUF as u32;
        for sn in 2..=last {
            let out = s.feed(&seg(sn, 0, b"z"));
            if sn < last {
                assert!(out.is_empty(), "sn {sn}");
            } else {
                assert_eq!(out.len(), Stream::MAX_RCV_BUF + 1);
            }
        }
        assert_eq!(s.gaps, 1);
    }

    #[test]
    fn sn_wraparound() {
        let mut s = Stream::default();
        assert_eq!(s.feed(&seg(u32::MAX, 0, b"a")), vec![b"a".to_vec()]);
        assert_eq!(s.feed(&seg(0, 0, b"b")), vec![b"b".to_vec()]);
    }
}
