//! Game message envelope inside a reassembled KCP message. Big-endian, unlike KCP.

pub const HEADER_SIZE: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    pub magic: [u8; 4],
    pub cmd_id: u16,
    pub head_len: u16,
    pub body_len: u32,
}

/// A parsed envelope borrowing the (still encrypted) body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message<'a> {
    pub header: MessageHeader,
    pub body: &'a [u8],
}

pub fn parse(b: &[u8]) -> Option<Message<'_>> {
    if b.len() < HEADER_SIZE {
        return None;
    }
    let header = MessageHeader {
        magic: [b[0], b[1], b[2], b[3]],
        cmd_id: u16::from_be_bytes([b[4], b[5]]),
        head_len: u16::from_be_bytes([b[6], b[7]]),
        body_len: u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
    };
    let start = HEADER_SIZE + usize::from(header.head_len);
    let end = start.checked_add(header.body_len as usize)?;
    Some(Message {
        header,
        body: b.get(start..end)?,
    })
}

pub fn write(cmd_id: u16, head: &[u8], body: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x01, 0x23, 0x45, 0x67]);
    out.extend_from_slice(&cmd_id.to_be_bytes());
    out.extend_from_slice(&(head.len() as u16).to_be_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(head);
    out.extend_from_slice(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_truncation() {
        let mut v = Vec::new();
        write(4937, b"hh", b"body!", &mut v);
        let m = parse(&v).unwrap();
        assert_eq!(m.header.cmd_id, 4937);
        assert_eq!(m.header.head_len, 2);
        assert_eq!(m.header.body_len, 5);
        assert_eq!(m.body, b"body!");
        assert!(parse(&v[..v.len() - 1]).is_none());
        assert!(parse(&v[..11]).is_none());
    }
}
