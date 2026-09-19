//! Fixture replay tests (testing.md T-F1, T-F2, T-F3, T-F6). Each
//! `tests/fixtures/<ver>-<region>-<desc>.pcapng` is replayed; when a matching
//! `.expect.json` (the reference tool's ZOD export) exists, our export must be
//! set-equal to it. Skipped, loudly, when no fixtures are present.

use std::path::{Path, PathBuf};

use hollow_proto::export::{export, ExportSettings, Zod};
use hollow_proto::fixture::{self, Packet};
use hollow_proto::gamedata::GameData;
use hollow_proto::model::PlayerData;
use hollow_proto::{Event, Pipeline, SessionState};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "pcapng"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    if v.is_empty() {
        eprintln!("no fixtures in {} — replay tests skipped", dir.display());
    }
    v
}

fn region_of(path: &Path) -> &str {
    // <ver>-<region>-<desc>.pcapng
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('-').nth(1))
        .expect("fixture name has a region")
}

fn run(packets: &[Packet], region: &str) -> (Vec<Event>, Pipeline) {
    let mut p = Pipeline::for_region(region).unwrap();
    let events = fixture::replay(&mut p, packets);
    (events, p)
}

fn collect(events: &[Event]) -> PlayerData {
    let mut d = PlayerData::default();
    for e in events {
        match e {
            Event::Agents(v) => d.agents.extend(v.iter().cloned()),
            Event::WEngines(v) => d.wengines.extend(v.iter().cloned()),
            Event::Discs(v) => d.discs.extend(v.iter().cloned()),
            _ => {}
        }
    }
    d
}

fn canonical(z: &Zod) -> Vec<String> {
    let mut out = Vec::new();
    for c in z.characters.iter().flatten() {
        out.push(serde_json::to_string(c).unwrap());
    }
    for w in z.wengines.iter().flatten() {
        let mut w = w.clone();
        w.lock = false; // reference never detects lock
        out.push(serde_json::to_string(&w).unwrap());
    }
    for d in z.discs.iter().flatten() {
        let mut d = d.clone();
        d.substats.retain(|s| !s.key.is_empty()); // reference pads to 4 empties
        d.lock = false; // reference never detects lock/trash
        d.trash = false;
        out.push(serde_json::to_string(&d).unwrap());
    }
    out.sort();
    out
}

#[test]
fn handshake_then_all_three_inventories() {
    for path in fixtures() {
        let packets = fixture::read(&path).unwrap();
        let (events, p) = run(&packets, region_of(&path));
        let idx = |pred: &dyn Fn(&Event) -> bool| events.iter().position(pred);
        let key = idx(&|e| matches!(e, Event::ServerKeyFound)).expect("ServerKeyFound");
        let est = idx(&|e| matches!(e, Event::SessionEstablished { .. })).expect("SessionEstablished");
        assert!(key < est, "{}", path.display());
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::SessionEstablished { .. }))
                .count(),
            1
        );
        assert_eq!(p.state(), SessionState::Established);
        let d = collect(&events);
        assert!(
            !d.agents.is_empty() && !d.wengines.is_empty() && !d.discs.is_empty(),
            "{}",
            path.display()
        );
        let warnings: Vec<_> = events.iter().filter(|e| matches!(e, Event::Warning(_))).collect();
        assert!(warnings.is_empty(), "{}: {warnings:?}", path.display());
        assert_eq!(p.stats().kcp_gaps, 0);
    }
}

#[test]
fn export_matches_reference_tool() {
    for path in fixtures() {
        let expect = path.with_extension("expect.json");
        let Ok(expect_json) = std::fs::read_to_string(&expect) else {
            continue;
        };
        let reference: Zod = serde_json::from_str(&expect_json).unwrap();
        let packets = fixture::read(&path).unwrap();
        let (events, _) = run(&packets, region_of(&path));
        let ours = export(&collect(&events), &GameData::vendored(), &ExportSettings::default());
        assert_eq!(canonical(&ours), canonical(&reference), "{}", path.display());
    }
}

#[test]
fn clock_skew_tolerance() {
    for path in fixtures() {
        let packets = fixture::read(&path).unwrap();
        let shift = |secs: i64| -> Vec<Packet> {
            packets
                .iter()
                .map(|p| Packet {
                    timestamp: std::time::Duration::from_secs((p.unix_secs() + secs) as u64),
                    ..p.clone()
                })
                .collect()
        };
        let (_, p) = run(&shift(3), region_of(&path));
        assert_eq!(p.state(), SessionState::Established, "+3s should still establish");
        let (events, p) = run(&shift(30), region_of(&path));
        assert_eq!(p.state(), SessionState::HaveServerKey, "+30s must not establish");
        assert!(events.iter().any(|e| matches!(e, Event::Warning(_))));
        assert!(collect(&events).discs.is_empty());
    }
}

#[test]
fn wrong_region_is_diagnosable() {
    for path in fixtures() {
        let packets = fixture::read(&path).unwrap();
        let other = if region_of(&path).eq_ignore_ascii_case("america") {
            "Europe"
        } else {
            "America"
        };
        let (events, p) = run(&packets, other);
        assert_eq!(p.state(), SessionState::Initial);
        assert!(collect(&events).discs.is_empty());
    }
}

#[test]
fn pcapng_json_interop() {
    for path in fixtures() {
        let packets = fixture::read(&path).unwrap();
        let dir = std::env::temp_dir().join("hollow-proto-replay");
        std::fs::create_dir_all(&dir).unwrap();
        let json = dir.join("interop.json");
        let pcap = dir.join("interop.pcapng");
        fixture::write_json(&json, &packets).unwrap();
        fixture::write_pcapng(&pcap, &packets).unwrap();
        let via_json = fixture::read(&json).unwrap();
        let via_pcap = fixture::read(&pcap).unwrap();
        assert_eq!(via_pcap, packets);
        let (a, _) = run(&via_json, region_of(&path));
        let (b, _) = run(&packets, region_of(&path));
        assert_eq!(a, b);
    }
}
