//! Field-number-driven decoding of the three inventory responses.

use crate::model::*;
use crate::proto::datamine::Datamine;
use crate::proto::wire::{self, Field};

/// What a decoded command carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Agents(Vec<Agent>),
    WEngines(Vec<WEngine>),
    Discs(Vec<DriveDisc>),
}

pub fn decode(dm: &Datamine, cmd_id: u16, fields: &[Field]) -> Option<Decoded> {
    if cmd_id == dm.cmd_get_avatar_data_sc_rsp {
        Some(Decoded::Agents(
            nested(fields, dm.agent_data.agents).map(|f| agent(dm, &f)).collect(),
        ))
    } else if cmd_id == dm.cmd_get_weapon_data_sc_rsp {
        Some(Decoded::WEngines(
            nested(fields, dm.weapon_data.weapons)
                .map(|f| wengine(dm, &f))
                .collect(),
        ))
    } else if cmd_id == dm.cmd_get_equip_data_sc_rsp {
        Some(Decoded::Discs(
            nested(fields, dm.equip_data.discs).map(|f| disc(dm, &f)).collect(),
        ))
    } else {
        None
    }
}

/// All parseable sub-messages stored under `number`.
fn nested(fields: &[Field], number: u32) -> impl Iterator<Item = Vec<Field>> + '_ {
    fields
        .iter()
        .filter(move |f| f.number == number)
        .filter_map(|f| f.bytes())
        .filter_map(|b| wire::parse(b).ok())
}

fn u32_field(fields: &[Field], number: u32) -> u32 {
    fields
        .iter()
        .find(|f| f.number == number)
        .and_then(Field::varint)
        .unwrap_or(0) as u32
}

pub fn agent(dm: &Datamine, f: &[Field]) -> Agent {
    let a = &dm.agent_info;
    Agent {
        id: u32_field(f, a.id),
        level: u32_field(f, a.level),
        promotion: u32_field(f, a.promotion),
        weapon_uid: u32_field(f, a.weapon_uid),
        mindscape: u32_field(f, a.mindscape),
        skills: nested(f, a.skills)
            .map(|s| SkillLevel {
                skill_type: u32_field(&s, dm.agent_skill.skill_type),
                level: u32_field(&s, dm.agent_skill.level),
            })
            .collect(),
        dressed_equips: nested(f, a.dressed_equips)
            .map(|e| DressedEquip {
                uid: u32_field(&e, dm.agent_equip.uid),
                slot: u32_field(&e, dm.agent_equip.slot),
            })
            .collect(),
    }
}

pub fn wengine(dm: &Datamine, f: &[Field]) -> WEngine {
    let w = &dm.weapon_info;
    WEngine {
        id: u32_field(f, w.id),
        uid: u32_field(f, w.uid),
        level: u32_field(f, w.level),
        phase: u32_field(f, w.phase),
        modification: u32_field(f, w.modification),
    }
}

fn disc_stat(dm: &Datamine, f: &[Field]) -> DiscStat {
    let s = &dm.disc_stat;
    DiscStat {
        key: u32_field(f, s.key),
        base_value: u32_field(f, s.base_value),
        add_value: u32_field(f, s.add_value),
    }
}

pub fn disc(dm: &Datamine, f: &[Field]) -> DriveDisc {
    let d = &dm.disc_info;
    DriveDisc {
        uid: u32_field(f, d.uid),
        id: u32_field(f, d.id),
        level: u32_field(f, d.level),
        main_stat: nested(f, d.main_stat)
            .next()
            .map(|s| disc_stat(dm, &s))
            .unwrap_or_default(),
        sub_stats: nested(f, d.sub_stats).map(|s| disc_stat(dm, &s)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::wire::{message, varint};

    #[test]
    fn discs_with_varying_substat_counts() {
        let dm = Datamine::vendored();
        let d = &dm.disc_info;
        let s = &dm.disc_stat;
        let stat = |key, add| {
            message(
                d.sub_stats,
                &[varint(s.key, key), varint(s.base_value, 10), varint(s.add_value, add)],
            )
        };
        let mut fields = Vec::new();
        for n in 2..=4u64 {
            let mut inner = vec![
                varint(d.uid, 1000 + n),
                varint(d.id, 31543),
                varint(d.level, 15),
                message(d.main_stat, &[varint(s.key, 20103), varint(s.base_value, 1)]),
                varint(99, 7), // unknown field, ignored
            ];
            inner.extend((0..n).map(|i| stat(11102 + i, i)));
            fields.push(message(dm.equip_data.discs, &inner));
        }
        fields.push(varint(1, 3)); // unrelated top-level field

        let Some(Decoded::Discs(discs)) = decode(&dm, dm.cmd_get_equip_data_sc_rsp, &fields) else {
            panic!()
        };
        assert_eq!(discs.len(), 3);
        assert_eq!(discs[0].sub_stats.len(), 2);
        assert_eq!(discs[2].sub_stats.len(), 4);
        assert_eq!(
            discs[2].sub_stats[3],
            DiscStat {
                key: 11105,
                base_value: 10,
                add_value: 3
            }
        );
        assert_eq!(discs[1].main_stat.key, 20103);
        assert_eq!((discs[1].uid, discs[1].id, discs[1].level), (1003, 31543, 15));
    }

    #[test]
    fn agents_and_wengines() {
        let dm = Datamine::vendored();
        let a = &dm.agent_info;
        let agent_msg = message(
            dm.agent_data.agents,
            &[
                varint(a.id, 1011),
                varint(a.level, 60),
                varint(a.promotion, 6),
                varint(a.weapon_uid, 77),
                varint(a.mindscape, 2),
                message(
                    a.skills,
                    &[varint(dm.agent_skill.skill_type, 5), varint(dm.agent_skill.level, 7)],
                ),
                message(
                    a.skills,
                    &[varint(dm.agent_skill.skill_type, 0), varint(dm.agent_skill.level, 12)],
                ),
                message(
                    a.dressed_equips,
                    &[varint(dm.agent_equip.uid, 5), varint(dm.agent_equip.slot, 1)],
                ),
            ],
        );
        let Some(Decoded::Agents(agents)) = decode(&dm, dm.cmd_get_avatar_data_sc_rsp, &[agent_msg]) else {
            panic!()
        };
        assert_eq!(agents.len(), 1);
        let ag = &agents[0];
        assert_eq!(
            (ag.id, ag.level, ag.promotion, ag.weapon_uid, ag.mindscape),
            (1011, 60, 6, 77, 2)
        );
        assert_eq!(ag.skill(skill_type::CORE), Some(7));
        assert_eq!(ag.skill(skill_type::BASIC), Some(12));
        assert_eq!(ag.skill(skill_type::ASSIST), None);
        assert_eq!(ag.dressed_equips, vec![DressedEquip { uid: 5, slot: 1 }]);

        let w = &dm.weapon_info;
        let wmsg = message(
            dm.weapon_data.weapons,
            &[
                varint(w.id, 12001),
                varint(w.uid, 77),
                varint(w.level, 60),
                varint(w.phase, 5),
                varint(w.modification, 5),
            ],
        );
        let Some(Decoded::WEngines(ws)) = decode(&dm, dm.cmd_get_weapon_data_sc_rsp, &[wmsg]) else {
            panic!()
        };
        assert_eq!(
            ws,
            vec![WEngine {
                id: 12001,
                uid: 77,
                level: 60,
                phase: 5,
                modification: 5
            }]
        );
    }

    #[test]
    fn unknown_cmd() {
        let dm = Datamine::vendored();
        assert_eq!(decode(&dm, 1, &[varint(1, 1)]), None);
    }
}
