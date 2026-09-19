//! Correlate the unmapped bool fields on discs / weapons with level and equipped-ness.
use hollow_proto::proto::datamine::Datamine;
use hollow_proto::proto::schema::Schema;
use hollow_proto::proto::wire::{self, Field};
use hollow_proto::{fixture, pipeline::RawPipeline};
use std::collections::{BTreeMap, HashSet};

fn u(f: &[Field], n: u32) -> u64 {
    f.iter().find(|x| x.number == n).and_then(Field::varint).unwrap_or(0)
}

fn main() {
    let path = std::env::args().nth(1).expect("fixture");
    let region = std::env::args().nth(2).unwrap_or_else(|| "America".into());
    let packets = fixture::read(std::path::Path::new(&path)).unwrap();
    let dm = Datamine::vendored();
    let mut pipe = RawPipeline::new(
        dm.region_seed(&region).unwrap(),
        dm.clone(),
        Schema::from_json(hollow_proto::vendored::NAP).unwrap(),
    );
    let mut discs: Vec<Vec<Field>> = vec![];
    let mut weapons: Vec<Vec<Field>> = vec![];
    let mut equipped: HashSet<u64> = HashSet::new();
    for pk in &packets {
        for (cmd, fields) in pipe.feed_raw(&pk.payload, pk.direction, pk.unix_secs()) {
            let sub = |no: u32| {
                fields
                    .iter()
                    .filter(move |f| f.number == no)
                    .filter_map(|f| f.bytes())
                    .filter_map(|b| wire::parse(b).ok())
            };
            if cmd == dm.cmd_get_equip_data_sc_rsp {
                discs.extend(sub(dm.equip_data.discs));
            } else if cmd == dm.cmd_get_weapon_data_sc_rsp {
                weapons.extend(sub(dm.weapon_data.weapons));
            } else if cmd == dm.cmd_get_avatar_data_sc_rsp {
                for a in sub(dm.agent_data.agents) {
                    equipped.insert(u(&a, dm.agent_info.weapon_uid));
                    for e in a
                        .iter()
                        .filter(|f| f.number == dm.agent_info.dressed_equips)
                        .filter_map(|f| f.bytes())
                        .filter_map(|b| wire::parse(b).ok())
                    {
                        equipped.insert(u(&e, dm.agent_equip.uid));
                    }
                }
            }
        }
    }
    let report = |label: &str, items: &[Vec<Field>], flag: u32, level: u32, uid: u32| {
        let set: Vec<&Vec<Field>> = items.iter().filter(|f| u(f, flag) == 1).collect();
        let mut lv: BTreeMap<u64, usize> = BTreeMap::new();
        let mut eq = 0;
        for f in &set {
            *lv.entry(u(f, level)).or_default() += 1;
            if equipped.contains(&u(f, uid)) {
                eq += 1;
            }
        }
        println!(
            "{label} field {flag}: {} set, {} of them equipped, level dist {:?}",
            set.len(),
            eq,
            lv
        );
    };
    println!(
        "{} discs, {} weapons, {} equipped uids",
        discs.len(),
        weapons.len(),
        equipped.len()
    );
    for flag in [5, 7, 10] {
        report("disc", &discs, flag, dm.disc_info.level, dm.disc_info.uid);
    }
    let unset: Vec<_> = discs.iter().filter(|f| u(f, 5) == 0).collect();
    println!(
        "disc field 5 UNSET: {} discs, {} equipped, level>0: {}",
        unset.len(),
        unset
            .iter()
            .filter(|f| equipped.contains(&u(f, dm.disc_info.uid)))
            .count(),
        unset.iter().filter(|f| u(f, dm.disc_info.level) > 0).count()
    );
    for flag in [4, 9] {
        report("weapon", &weapons, flag, dm.weapon_info.level, dm.weapon_info.uid);
    }
    let unset: Vec<_> = weapons.iter().filter(|f| u(f, 4) == 0).collect();
    println!(
        "weapon field 4 UNSET: {:?}",
        unset
            .iter()
            .map(|f| (u(f, dm.weapon_info.id), u(f, dm.weapon_info.level)))
            .collect::<Vec<_>>()
    );
}
