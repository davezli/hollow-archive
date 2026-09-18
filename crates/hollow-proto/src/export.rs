//! Zenless Optimizer "ZOD" v1 export. Shape copied from zzz_packet_capture
//! `src/serialization/zod/*` — see specs/implementation.md §1.10.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::gamedata::GameData;
use crate::model::{skill_type, PlayerData};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub export_agents: bool,
    pub export_discs: bool,
    pub export_wengines: bool,
    pub min_agent_rarity: u32,
    pub min_agent_level: u32,
    pub min_disc_rarity: u32,
    pub min_disc_level: u32,
    pub min_wengine_rarity: u32,
    pub min_wengine_level: u32,
    /// Emit exactly four substat entries, padding with empty keys, as the
    /// reference tool does. Off by default (D5).
    pub pad_substats: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            export_agents: true,
            export_discs: true,
            export_wengines: true,
            min_agent_rarity: 4,
            min_agent_level: 0,
            min_disc_rarity: 3,
            min_disc_level: 0,
            min_wengine_rarity: 3,
            min_wengine_level: 0,
            pad_substats: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZodAgent {
    /// Zenless Optimizer resets all skills to 1 if this key is absent.
    pub equipped_engine: String,
    pub key: String,
    pub level: u32,
    pub mindscape: u32,
    pub promotion: u32,
    pub core: u32,
    pub dodge: u32,
    pub basic: u32,
    pub chain: u32,
    pub special: u32,
    pub assist: u32,
    pub potential: u32,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZodSubstat {
    pub key: String,
    pub upgrades: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZodDisc {
    pub set_key: String,
    pub slot_key: String,
    pub level: u32,
    pub rarity: String,
    pub main_stat_key: String,
    pub location: String,
    pub lock: bool,
    pub trash: bool,
    pub substats: Vec<ZodSubstat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZodWEngine {
    pub key: String,
    pub level: u32,
    pub modification: u32,
    pub phase: u32,
    pub location: String,
    pub lock: bool,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zod {
    pub format: String,
    pub version: u32,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characters: Option<Vec<ZodAgent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discs: Option<Vec<ZodDisc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wengines: Option<Vec<ZodWEngine>>,
}

pub const SOURCE: &str = "Hollow Archive";

/// Stat id -> ZOD stat key (zzz_packet_capture `statMap.hpp`).
pub fn stat_key(id: u32) -> Option<&'static str> {
    Some(match id {
        11102 => "hp_",
        11103 => "hp",
        12102 => "atk_",
        12103 => "atk",
        12202 => "impact_",
        13102 => "def_",
        13103 => "def",
        20103 => "crit_",
        21103 => "crit_dmg_",
        23103 => "pen_",
        23203 => "pen",
        30502 => "enerRegen_",
        31203 => "anomProf",
        31402 => "anomMas_",
        31503 => "physical_dmg_",
        31603 => "fire_dmg_",
        31703 => "ice_dmg_",
        31803 => "electric_dmg_",
        31903 => "ether_dmg_",
        32303 => "wind_dmg_",
        _ => return None,
    })
}

pub fn rarity_key(r: u32) -> &'static str {
    match r {
        3 => "B",
        4 => "A",
        5 => "S",
        _ => "?",
    }
}

/// `toZodKey`: drop apostrophes and hyphens, treat any other non-alphanumeric
/// as a word break, UpperCamelCase the words, join without spaces.
pub fn to_zod_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut at_word_start = true;
    for c in name.chars() {
        if c == '\'' || c == '-' {
            continue;
        }
        if !c.is_alphanumeric() {
            at_word_start = true;
            continue;
        }
        if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn unknown(kind: &str, id: u32) -> String {
    format!("Unknown{kind}{id}")
}

pub fn export(data: &PlayerData, gd: &GameData, s: &ExportSettings) -> Zod {
    let agent_key = |id: u32| {
        gd.agent_name(id)
            .map(to_zod_key)
            .unwrap_or_else(|| unknown("Agent", id))
    };

    // uid -> wearer, for `location`.
    let mut engine_loc: HashMap<u32, String> = HashMap::new();
    let mut disc_loc: HashMap<u32, String> = HashMap::new();
    for a in &data.agents {
        let key = agent_key(a.id);
        engine_loc.insert(a.weapon_uid, key.clone());
        for e in &a.dressed_equips {
            disc_loc.insert(e.uid, key.clone());
        }
    }

    let characters = s.export_agents.then(|| {
        data.agents
            .iter()
            .filter(|a| a.level >= s.min_agent_level && gd.agent_rarity(a.id).unwrap_or(0) >= s.min_agent_rarity)
            .map(|a| {
                let key = agent_key(a.id);
                let sk = |t| a.skill(t).unwrap_or(0);
                ZodAgent {
                    equipped_engine: String::new(),
                    id: key.clone(),
                    key,
                    level: a.level,
                    mindscape: a.mindscape,
                    promotion: a.promotion.saturating_sub(1),
                    core: sk(skill_type::CORE).saturating_sub(1),
                    dodge: sk(skill_type::DODGE),
                    basic: sk(skill_type::BASIC),
                    chain: sk(skill_type::CHAIN),
                    special: sk(skill_type::SPECIAL),
                    assist: sk(skill_type::ASSIST),
                    potential: 0,
                }
            })
            .collect()
    });

    let discs = s.export_discs.then(|| {
        data.discs
            .iter()
            .filter(|d| d.level >= s.min_disc_level && d.rarity() >= s.min_disc_rarity)
            .map(|d| {
                let mut substats: Vec<ZodSubstat> = d
                    .sub_stats
                    .iter()
                    .map(|st| ZodSubstat {
                        key: stat_key(st.key).unwrap_or("").to_string(),
                        upgrades: st.add_value,
                    })
                    .collect();
                if s.pad_substats {
                    substats.resize_with(4, || ZodSubstat {
                        key: String::new(),
                        upgrades: 0,
                    });
                }
                ZodDisc {
                    set_key: gd
                        .set_name(d.set_id())
                        .map(to_zod_key)
                        .unwrap_or_else(|| unknown("Set", d.set_id())),
                    slot_key: d.slot().to_string(),
                    level: d.level,
                    rarity: rarity_key(d.rarity()).to_string(),
                    main_stat_key: stat_key(d.main_stat.key).unwrap_or("").to_string(),
                    location: disc_loc.get(&d.uid).cloned().unwrap_or_default(),
                    lock: false,
                    trash: false,
                    substats,
                }
            })
            .collect()
    });

    let wengines = s.export_wengines.then(|| {
        data.wengines
            .iter()
            .filter(|w| w.level >= s.min_wengine_level && gd.wengine_rarity(w.id).unwrap_or(0) >= s.min_wengine_rarity)
            .map(|w| ZodWEngine {
                key: gd
                    .wengine_name(w.id)
                    .map(to_zod_key)
                    .unwrap_or_else(|| unknown("WEngine", w.id)),
                level: w.level,
                modification: w.modification,
                phase: w.phase,
                location: engine_loc.get(&w.uid).cloned().unwrap_or_default(),
                lock: false,
                id: format!("zzz_wengine_{}", w.uid),
            })
            .collect()
    });

    Zod {
        format: "ZOD".into(),
        version: 1,
        source: SOURCE.into(),
        characters,
        discs,
        wengines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn zod_keys() {
        for (name, key) in [
            ("Zhu Yuan", "ZhuYuan"),
            ("Soldier 11", "Soldier11"),
            ("Nekomiya Mana", "NekomiyaMana"),
            ("Von Lycaon", "VonLycaon"),
            ("[Lunar] Pleniluna", "LunarPleniluna"),
            ("Sharpshooter's Gaze", "SharpshootersGaze"),
            ("Demara Battery Mark II", "DemaraBatteryMarkII"),
            ("Big Cylinder", "BigCylinder"),
            ("  spaced   out ", "SpacedOut"),
        ] {
            assert_eq!(to_zod_key(name), key, "{name}");
        }
    }

    fn sample() -> PlayerData {
        PlayerData {
            agents: vec![Agent {
                id: 1011,
                level: 20,
                promotion: 2,
                weapon_uid: 7,
                mindscape: 6,
                skills: [0, 1, 2, 3, 5, 6]
                    .map(|t| SkillLevel {
                        skill_type: t,
                        level: if t == 5 { 2 } else { 1 },
                    })
                    .to_vec(),
                dressed_equips: vec![DressedEquip { uid: 100, slot: 1 }],
            }],
            wengines: vec![
                WEngine {
                    id: 12001,
                    uid: 7,
                    level: 30,
                    phase: 5,
                    modification: 2,
                },
                WEngine {
                    id: 12001,
                    uid: 8,
                    level: 1,
                    phase: 1,
                    modification: 1,
                },
            ],
            discs: vec![
                DriveDisc {
                    uid: 100,
                    id: 31021, // Woodpecker Electro, B, slot 1
                    level: 0,
                    main_stat: DiscStat {
                        key: 11103,
                        base_value: 550,
                        add_value: 0,
                    },
                    sub_stats: vec![DiscStat {
                        key: 13102,
                        base_value: 1,
                        add_value: 1,
                    }],
                },
                DriveDisc {
                    uid: 101,
                    id: 31044, // A, slot 4
                    level: 9,
                    main_stat: DiscStat {
                        key: 20103,
                        base_value: 1,
                        add_value: 0,
                    },
                    sub_stats: vec![],
                },
            ],
        }
    }

    #[test]
    fn defaults_match_reference_shape() {
        let z = export(&sample(), &GameData::vendored(), &ExportSettings::default());
        let c = &z.characters.as_ref().unwrap()[0];
        assert_eq!(c.key, "Anby");
        assert_eq!(c.id, "Anby");
        assert_eq!(c.equipped_engine, "");
        assert_eq!((c.promotion, c.core, c.basic, c.special), (1, 1, 1, 1));

        let d = &z.discs.as_ref().unwrap()[0];
        assert_eq!(
            (d.set_key.as_str(), d.slot_key.as_str(), d.rarity.as_str()),
            ("WoodpeckerElectro", "1", "B")
        );
        assert_eq!(d.main_stat_key, "hp");
        assert_eq!(d.location, "Anby");
        assert_eq!(
            d.substats,
            vec![ZodSubstat {
                key: "def_".into(),
                upgrades: 1
            }]
        );
        assert_eq!(z.discs.as_ref().unwrap()[1].location, "");

        let w = &z.wengines.as_ref().unwrap()[0];
        assert_eq!(
            (w.key.as_str(), w.location.as_str(), w.id.as_str()),
            ("LunarPleniluna", "Anby", "zzz_wengine_7")
        );
        assert_eq!(z.wengines.as_ref().unwrap()[1].location, "");
    }

    #[test]
    fn filters_and_toggles() {
        let gd = GameData::vendored();
        let s = ExportSettings {
            export_discs: false,
            min_wengine_level: 10,
            pad_substats: true,
            ..Default::default()
        };
        let z = export(&sample(), &gd, &s);
        assert!(z.discs.is_none());
        assert_eq!(z.wengines.unwrap().len(), 1);
        let json = serde_json::to_string(&export(
            &sample(),
            &gd,
            &ExportSettings {
                export_agents: false,
                ..s.clone()
            },
        ))
        .unwrap();
        assert!(!json.contains("\"characters\""));

        let padded = export(
            &sample(),
            &gd,
            &ExportSettings {
                pad_substats: true,
                ..Default::default()
            },
        );
        assert_eq!(padded.discs.unwrap()[0].substats.len(), 4);

        let z = export(
            &sample(),
            &gd,
            &ExportSettings {
                min_disc_rarity: 4,
                ..Default::default()
            },
        );
        assert_eq!(z.discs.unwrap().len(), 1);
        let z = export(
            &sample(),
            &gd,
            &ExportSettings {
                min_agent_level: 21,
                ..Default::default()
            },
        );
        assert!(z.characters.unwrap().is_empty());
    }
}
