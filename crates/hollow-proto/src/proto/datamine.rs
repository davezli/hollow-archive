//! `datamine.json`: command IDs, protobuf field numbers and per-region initial
//! XOR seeds for one game version. Field names mirror the upstream file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{ProtoError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillFields {
    pub skill_type: u32,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DressedEquipFields {
    pub uid: u32,
    pub slot: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfoFields {
    pub id: u32,
    pub level: u32,
    pub promotion: u32,
    #[serde(rename = "weaponUid")]
    pub weapon_uid: u32,
    /// Not present in upstream datamine.json as of 3.2; core level comes from the skills list.
    #[serde(default)]
    pub core: u32,
    pub mindscape: u32,
    pub skills: u32,
    pub dressed_equips: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDataFields {
    pub agents: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipDataFields {
    pub discs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDataFields {
    pub weapons: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscStatFields {
    pub key: u32,
    pub base_value: u32,
    pub add_value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscInfoFields {
    pub uid: u32,
    pub id: u32,
    pub level: u32,
    #[serde(rename = "mainStat")]
    pub main_stat: u32,
    #[serde(rename = "subStats")]
    pub sub_stats: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponInfoFields {
    pub id: u32,
    pub uid: u32,
    pub level: u32,
    pub phase: u32,
    pub modification: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Datamine {
    /// Region name -> 16-hex-digit initial XOR seed.
    pub xor_seeds: BTreeMap<String, String>,
    pub cmd_player_get_token_sc_rsp: u16,
    pub cmd_get_equip_data_sc_rsp: u16,
    pub cmd_get_weapon_data_sc_rsp: u16,
    pub cmd_get_avatar_data_sc_rsp: u16,
    pub agent_data: AgentDataFields,
    pub agent_info: AgentInfoFields,
    pub agent_skill: AgentSkillFields,
    pub agent_equip: DressedEquipFields,
    pub equip_data: EquipDataFields,
    pub disc_info: DiscInfoFields,
    pub disc_stat: DiscStatFields,
    pub weapon_data: WeaponDataFields,
    pub weapon_info: WeaponInfoFields,
}

impl Datamine {
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn vendored() -> Self {
        Self::from_json(crate::vendored::DATAMINE).expect("vendored datamine.json is valid")
    }

    /// Region names as listed in the data file (UI picker order).
    pub fn regions(&self) -> impl Iterator<Item = &str> {
        self.xor_seeds.keys().map(String::as_str)
    }

    /// Initial XOR seed for a region, matched case-insensitively and ignoring
    /// spaces so "america", "TW,HK,MO" and "twhkmo" all work.
    pub fn region_seed(&self, region: &str) -> Result<u64> {
        let norm = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
        };
        let want = norm(region);
        self.xor_seeds
            .iter()
            .find(|(k, _)| norm(k) == want)
            .and_then(|(_, v)| u64::from_str_radix(v, 16).ok())
            .ok_or_else(|| ProtoError::UnknownRegion(region.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_is_sane() {
        let d = Datamine::vendored();
        assert_eq!(d.cmd_player_get_token_sc_rsp, 4937);
        assert_eq!(d.region_seed("America").unwrap(), 0x50C2_1982_AC00_9AF2);
        assert_eq!(d.region_seed("tw,hk,mo").unwrap(), d.region_seed("TWHKMO").unwrap());
        assert!(d.region_seed("Mars").is_err());
        for n in [
            d.agent_data.agents,
            d.agent_info.id,
            d.agent_info.level,
            d.agent_info.skills,
            d.agent_skill.level,
            d.agent_equip.uid,
            d.equip_data.discs,
            d.disc_info.id,
            d.disc_stat.key,
            d.weapon_data.weapons,
            d.weapon_info.id,
        ] {
            assert_ne!(n, 0);
        }
    }
}
