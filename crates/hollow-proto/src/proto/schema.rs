//! `nap.json`: the obfuscated proto schema dumped from the client. We use it for
//! exactly one thing — the per-field XOR values the client applies on top of
//! the stream cipher.

use std::collections::HashMap;

use serde::Deserialize;

use super::wire::{self, Field, Value};
use crate::error::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaField {
    pub number: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub xor_value: Option<u64>,
    #[serde(default)]
    pub is_native_type: bool,
    #[serde(default)]
    pub is_enum: bool,
    #[serde(default)]
    pub repeated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaEntry {
    pub name: String,
    #[serde(default)]
    pub cmd_id: Option<u16>,
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Default)]
pub struct Schema {
    entries: Vec<SchemaEntry>,
    by_cmd: HashMap<u16, usize>,
    by_name: HashMap<String, usize>,
}

impl Schema {
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(Self::from_entries(serde_json::from_str(json)?))
    }

    pub fn from_entries(entries: Vec<SchemaEntry>) -> Self {
        let mut s = Schema {
            entries,
            ..Default::default()
        };
        for (i, e) in s.entries.iter().enumerate() {
            if let Some(c) = e.cmd_id {
                s.by_cmd.insert(c, i);
            }
            s.by_name.insert(e.name.clone(), i);
        }
        s
    }

    pub fn by_cmd(&self, cmd_id: u16) -> Option<&SchemaEntry> {
        self.by_cmd.get(&cmd_id).map(|&i| &self.entries[i])
    }

    pub fn by_name(&self, name: &str) -> Option<&SchemaEntry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Undo field-level XOR obfuscation for a message of command `cmd_id`, in place.
    /// Unknown commands are left untouched.
    pub fn unxor_cmd(&self, cmd_id: u16, fields: &mut [Field]) {
        if let Some(entry) = self.by_cmd(cmd_id) {
            self.unxor(entry, fields);
        }
    }

    pub fn unxor(&self, entry: &SchemaEntry, fields: &mut [Field]) {
        for f in fields.iter_mut() {
            let Some(sf) = entry.fields.iter().find(|sf| sf.number == f.number) else {
                continue;
            };
            if !sf.is_native_type && !sf.is_enum {
                // Nested message: recurse by type name (maps and unknown types are no-ops).
                if let (Value::Bytes(b), Some(nested)) = (&mut f.value, self.by_name(&sf.type_name)) {
                    if let Ok(mut inner) = wire::parse(b) {
                        self.unxor(nested, &mut inner);
                        *b = wire::encode(&inner);
                    }
                }
                continue;
            }
            let Some(x) = sf.xor_value.filter(|&x| x != 0) else {
                continue;
            };
            match &mut f.value {
                Value::Varint(v) => *v ^= x,
                Value::Fixed32(v) => *v ^= x as u32,
                Value::Fixed64(v) => *v ^= x,
                Value::Bytes(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::wire::{message, varint};

    fn schema() -> Schema {
        Schema::from_json(
            r#"[
              {"name":"Outer","cmd_id":42,"fields":[
                {"number":1,"name":"a","type":"uint32","xor_value":5927,"is_native_type":true},
                {"number":2,"name":"b","type":"uint32","xor_value":0,"is_native_type":true},
                {"number":3,"name":"n","type":"Inner","is_native_type":false},
                {"number":4,"name":"m","type":"map<int32, bool>","is_native_type":false}
              ]},
              {"name":"Inner","cmd_id":null,"fields":[
                {"number":9,"name":"x","type":"int32","xor_value":7,"is_native_type":true}
              ]}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn unxor_top_and_nested() {
        let s = schema();
        let mut fields = vec![
            varint(1, 100 ^ 5927),
            varint(2, 100),
            message(3, &[varint(9, 3 ^ 7), varint(10, 1)]),
            message(4, &[varint(1, 1)]),
            varint(5, 55),
        ];
        s.unxor_cmd(42, &mut fields);
        assert_eq!(fields[0], varint(1, 100));
        assert_eq!(fields[1], varint(2, 100));
        assert_eq!(fields[2], message(3, &[varint(9, 3), varint(10, 1)]));
        assert_eq!(fields[3], message(4, &[varint(1, 1)]));
        assert_eq!(fields[4], varint(5, 55));
    }

    #[test]
    fn unknown_cmd_is_noop() {
        let s = schema();
        let mut fields = vec![varint(1, 1)];
        s.unxor_cmd(43, &mut fields);
        assert_eq!(fields, vec![varint(1, 1)]);
    }

    #[test]
    fn vendored_nap_parses() {
        let s = Schema::from_json(crate::vendored::NAP).unwrap();
        assert!(s.len() > 1000);
        let dm = crate::proto::datamine::Datamine::from_json(crate::vendored::DATAMINE).unwrap();
        assert!(s.by_cmd(dm.cmd_get_equip_data_sc_rsp).is_some());
    }
}
