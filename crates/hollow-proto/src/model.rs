//! Decoded player data, as it appears on the wire (no name resolution).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLevel {
    pub skill_type: u32,
    pub level: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DressedEquip {
    pub uid: u32,
    pub slot: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: u32,
    pub level: u32,
    pub promotion: u32,
    pub weapon_uid: u32,
    pub mindscape: u32,
    pub skills: Vec<SkillLevel>,
    pub dressed_equips: Vec<DressedEquip>,
}

/// `skill_type` values as seen on the wire (3.2). Type 4 is never sent; the
/// reference tool relies on list position instead, which gives the same result.
pub mod skill_type {
    pub const BASIC: u32 = 0;
    pub const SPECIAL: u32 = 1;
    pub const DODGE: u32 = 2;
    pub const CHAIN: u32 = 3;
    pub const CORE: u32 = 5;
    pub const ASSIST: u32 = 6;
}

impl Agent {
    pub fn skill(&self, skill_type: u32) -> Option<u32> {
        self.skills.iter().find(|s| s.skill_type == skill_type).map(|s| s.level)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WEngine {
    pub id: u32,
    pub uid: u32,
    pub level: u32,
    pub phase: u32,
    pub modification: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscStat {
    pub key: u32,
    pub base_value: u32,
    /// For substats: number of upgrade rolls on top of the initial one.
    pub add_value: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveDisc {
    pub uid: u32,
    /// Encodes set, rarity and slot; see the accessors.
    pub id: u32,
    pub level: u32,
    pub main_stat: DiscStat,
    pub sub_stats: Vec<DiscStat>,
}

impl DriveDisc {
    /// 3 = B, 4 = A, 5 = S.
    pub fn rarity(&self) -> u32 {
        self.id / 10 % 10 + 1
    }
    /// 1..=6
    pub fn slot(&self) -> u32 {
        self.id % 10
    }
    /// Set id as keyed in the game data (e.g. 31000 for Woodpecker Electro).
    pub fn set_id(&self) -> u32 {
        self.id / 100 * 100
    }
}

/// Everything captured from one login.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerData {
    pub agents: Vec<Agent>,
    pub wengines: Vec<WEngine>,
    pub discs: Vec<DriveDisc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_id_decomposition() {
        let d = |id| DriveDisc {
            id,
            ..Default::default()
        };
        assert_eq!((d(31543).set_id(), d(31543).rarity(), d(31543).slot()), (31500, 5, 3));
        assert_eq!((d(31221).set_id(), d(31221).rarity(), d(31221).slot()), (31200, 3, 1));
        for rarity in 3..=5u32 {
            for slot in 1..=6u32 {
                let id = 32000 + (rarity - 1) * 10 + slot;
                assert_eq!((d(id).rarity(), d(id).slot(), d(id).set_id()), (rarity, slot, 32000));
            }
        }
    }
}
