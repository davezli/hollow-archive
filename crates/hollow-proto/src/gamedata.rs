//! ID -> name/rarity tables (slim snapshot of nanoka.cc data; see scripts/update-gamedata.py).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Named {
    pub en: String,
    /// 0-based rarity index: rank + 1 == 3 (B), 4 (A), 5 (S).
    #[serde(default)]
    pub rank: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetName {
    pub en: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    pub version: String,
    pub characters: BTreeMap<u32, Named>,
    pub equipment: BTreeMap<u32, SetName>,
    pub weapons: BTreeMap<u32, Named>,
}

impl GameData {
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn vendored() -> Self {
        Self::from_json(crate::vendored::NANOKA).expect("vendored nanoka.json is valid")
    }

    pub fn agent_name(&self, id: u32) -> Option<&str> {
        self.characters.get(&id).map(|n| n.en.as_str())
    }
    pub fn agent_rarity(&self, id: u32) -> Option<u32> {
        self.characters.get(&id).map(|n| n.rank + 1)
    }
    pub fn wengine_name(&self, id: u32) -> Option<&str> {
        self.weapons.get(&id).map(|n| n.en.as_str())
    }
    pub fn wengine_rarity(&self, id: u32) -> Option<u32> {
        self.weapons.get(&id).map(|n| n.rank + 1)
    }
    pub fn set_name(&self, set_id: u32) -> Option<&str> {
        self.equipment.get(&set_id).map(|n| n.en.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_lookups() {
        let g = GameData::vendored();
        assert_eq!(g.version, "3.2");
        assert_eq!(g.agent_name(1011), Some("Anby"));
        assert_eq!(g.agent_rarity(1011), Some(4));
        assert_eq!(g.set_name(31000), Some("Woodpecker Electro"));
        assert_eq!(g.wengine_name(12001), Some("[Lunar] Pleniluna"));
        assert_eq!(g.wengine_rarity(12001), Some(3));
        assert_eq!(g.agent_name(0), None);
    }
}
