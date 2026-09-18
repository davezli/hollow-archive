//! Minimal protobuf wire-format reader/writer. Schema-less: every message is a
//! flat list of `(field number, value)` pairs, the way `UnknownFieldSet` sees it.

use crate::error::{ProtoError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Varint(u64),
    Fixed64(u64),
    Fixed32(u32),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub number: u32,
    pub value: Value,
}

impl Field {
    pub fn varint(&self) -> Option<u64> {
        match self.value {
            Value::Varint(v) => Some(v),
            _ => None,
        }
    }
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.value {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

pub fn read_varint(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*pos).ok_or(ProtoError::Wire("truncated varint"))?;
        *pos += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(ProtoError::Wire("varint longer than 10 bytes"))
}

pub fn write_varint(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Parse a whole message. Fails on any malformed byte, which is what makes
/// "does it parse?" a usable oracle for the session-key brute force.
pub fn parse(b: &[u8]) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while pos < b.len() {
        let tag = read_varint(b, &mut pos)?;
        let number = u32::try_from(tag >> 3).map_err(|_| ProtoError::Wire("field number too large"))?;
        if number == 0 {
            return Err(ProtoError::Wire("field number 0"));
        }
        let value = match tag & 7 {
            0 => Value::Varint(read_varint(b, &mut pos)?),
            1 => {
                let s = b.get(pos..pos + 8).ok_or(ProtoError::Wire("truncated fixed64"))?;
                pos += 8;
                Value::Fixed64(u64::from_le_bytes(s.try_into().unwrap()))
            }
            2 => {
                let len =
                    usize::try_from(read_varint(b, &mut pos)?).map_err(|_| ProtoError::Wire("length overflow"))?;
                let end = pos.checked_add(len).ok_or(ProtoError::Wire("length overflow"))?;
                let s = b.get(pos..end).ok_or(ProtoError::Wire("truncated length-delimited"))?;
                pos = end;
                Value::Bytes(s.to_vec())
            }
            5 => {
                let s = b.get(pos..pos + 4).ok_or(ProtoError::Wire("truncated fixed32"))?;
                pos += 4;
                Value::Fixed32(u32::from_le_bytes(s.try_into().unwrap()))
            }
            3 | 4 => return Err(ProtoError::Wire("groups unsupported")),
            _ => return Err(ProtoError::Wire("bad wire type")),
        };
        fields.push(Field { number, value });
    }
    Ok(fields)
}

pub fn write(fields: &[Field], out: &mut Vec<u8>) {
    for f in fields {
        let (wt, _) = match f.value {
            Value::Varint(_) => (0, ()),
            Value::Fixed64(_) => (1, ()),
            Value::Bytes(_) => (2, ()),
            Value::Fixed32(_) => (5, ()),
        };
        write_varint((u64::from(f.number) << 3) | wt, out);
        match &f.value {
            Value::Varint(v) => write_varint(*v, out),
            Value::Fixed64(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::Fixed32(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::Bytes(b) => {
                write_varint(b.len() as u64, out);
                out.extend_from_slice(b);
            }
        }
    }
}

pub fn encode(fields: &[Field]) -> Vec<u8> {
    let mut v = Vec::new();
    write(fields, &mut v);
    v
}

/// Convenience constructors for building test messages.
pub fn varint(number: u32, v: u64) -> Field {
    Field {
        number,
        value: Value::Varint(v),
    }
}
pub fn bytes(number: u32, b: impl Into<Vec<u8>>) -> Field {
    Field {
        number,
        value: Value::Bytes(b.into()),
    }
}
pub fn message(number: u32, fields: &[Field]) -> Field {
    bytes(number, encode(fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_types() {
        let fields = vec![
            varint(1, 0),
            varint(2, 127),
            varint(3, 128),
            varint(4, u64::MAX),
            Field {
                number: 5,
                value: Value::Fixed64(0x0102030405060708),
            },
            Field {
                number: 6,
                value: Value::Fixed32(0xdeadbeef),
            },
            bytes(7, b"hi".to_vec()),
            message(8, &[varint(1, 5)]),
            varint(536_870_911, 1),
        ];
        assert_eq!(parse(&encode(&fields)).unwrap(), fields);
    }

    #[test]
    fn malformed() {
        assert!(parse(&[0x08]).is_err()); // tag, no value
        assert!(parse(&[0x08, 0x80]).is_err()); // truncated varint
        assert!(parse(&[0x12, 0x05, b'a']).is_err()); // length overrun
        assert!(parse(&[0x0e]).is_err()); // wire type 6
        assert!(parse(&[0x0b]).is_err()); // group
        assert!(parse(&[0x00]).is_err()); // field 0
        assert!(parse(&[]).unwrap().is_empty());
    }
}
