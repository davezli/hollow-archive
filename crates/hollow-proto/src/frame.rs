//! Strip link/network/transport headers from a captured frame down to the UDP payload.

/// A UDP datagram extracted from a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Udp<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

const ETH_IPV4: u16 = 0x0800;
const ETH_IPV6: u16 = 0x86DD;
const ETH_VLAN: u16 = 0x8100;
const IPPROTO_UDP: u8 = 17;

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

/// Parse an Ethernet II frame (optionally 802.1Q tagged) carrying IPv4/IPv6 + UDP.
/// Returns `None` for anything else (ARP, TCP, truncated, ...).
pub fn ethernet(frame: &[u8]) -> Option<Udp<'_>> {
    let mut ethertype = be16(frame, 12)?;
    let mut off = 14;
    if ethertype == ETH_VLAN {
        ethertype = be16(frame, 16)?;
        off = 18;
    }
    match ethertype {
        ETH_IPV4 => ipv4(frame.get(off..)?),
        ETH_IPV6 => ipv6(frame.get(off..)?),
        _ => None,
    }
}

/// Parse a packet starting at the IP header (either version).
pub fn ip(pkt: &[u8]) -> Option<Udp<'_>> {
    match pkt.first()? >> 4 {
        4 => ipv4(pkt),
        6 => ipv6(pkt),
        _ => None,
    }
}

pub fn ipv4(pkt: &[u8]) -> Option<Udp<'_>> {
    let ihl = usize::from(pkt.first()? & 0x0f) * 4;
    if ihl < 20 || *pkt.get(9)? != IPPROTO_UDP {
        return None;
    }
    let total_len = usize::from(be16(pkt, 2)?);
    let end = total_len.min(pkt.len());
    udp(pkt.get(ihl..end)?)
}

pub fn ipv6(pkt: &[u8]) -> Option<Udp<'_>> {
    // Extension headers are not handled; game traffic has none.
    if *pkt.get(6)? != IPPROTO_UDP {
        return None;
    }
    let payload_len = usize::from(be16(pkt, 4)?);
    let end = (40 + payload_len).min(pkt.len());
    udp(pkt.get(40..end)?)
}

pub fn udp(seg: &[u8]) -> Option<Udp<'_>> {
    let src_port = be16(seg, 0)?;
    let dst_port = be16(seg, 2)?;
    let len = usize::from(be16(seg, 4)?);
    if len < 8 {
        return None;
    }
    let end = len.min(seg.len());
    Some(Udp {
        src_port,
        dst_port,
        payload: seg.get(8..end)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(vlan: bool, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 12];
        if vlan {
            f.extend_from_slice(&[0x81, 0x00, 0x00, 0x01]);
        }
        f.extend_from_slice(&[0x08, 0x00]);
        let udp_len = 8 + payload.len();
        let total = 20 + udp_len;
        let mut ip = vec![0x45, 0, (total >> 8) as u8, total as u8, 0, 0, 0, 0, 64, 17, 0, 0];
        ip.extend_from_slice(&[10, 0, 0, 1, 10, 0, 0, 2]);
        f.extend_from_slice(&ip);
        f.extend_from_slice(&[0x12, 0x34, 0x50, 0x15, (udp_len >> 8) as u8, udp_len as u8, 0, 0]);
        f.extend_from_slice(payload);
        f
    }

    #[test]
    fn plain_and_vlan() {
        for vlan in [false, true] {
            let f = build(vlan, b"hello");
            let u = ethernet(&f).unwrap();
            assert_eq!((u.src_port, u.dst_port, u.payload), (0x1234, 20501, &b"hello"[..]));
        }
    }

    #[test]
    fn trailing_padding_is_trimmed() {
        let mut f = build(false, b"abc");
        f.extend_from_slice(&[0; 20]);
        assert_eq!(ethernet(&f).unwrap().payload, b"abc");
    }

    #[test]
    fn non_udp_and_truncated() {
        let mut f = build(false, b"abc");
        f[23] = 6; // TCP
        assert!(ethernet(&f).is_none());
        assert!(ethernet(&build(false, b"abc")[..30]).is_none());
        assert!(ethernet(&[]).is_none());
    }

    #[test]
    fn ipv6_datagram() {
        let payload = b"zz";
        let mut p = vec![0x60, 0, 0, 0, 0, 8 + 2, 17, 64];
        p.extend_from_slice(&[0; 32]);
        p.extend_from_slice(&[0x50, 0x15, 0x00, 0x01, 0, 10, 0, 0]);
        p.extend_from_slice(payload);
        let u = ip(&p).unwrap();
        assert_eq!((u.src_port, u.dst_port, u.payload), (20501, 1, &payload[..]));
    }
}
