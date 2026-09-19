//! Data files: vendored fallback -> local cache -> upstream update.
//!
//! `datamine.json` / `nap.json` / `manifest.json` come from zzz_packet_capture's
//! repo (they re-dump them each game patch); names come from nanoka.cc. Updates
//! land in `%LOCALAPPDATA%\hollow-archive\data\` and win over the vendored copies
//! when their version is newer.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use hollow_proto::gamedata::GameData;
use hollow_proto::proto::datamine::Datamine;
use hollow_proto::proto::schema::Schema;
use hollow_proto::vendored;
use serde::Deserialize;

const UPSTREAM: &str = "https://raw.githubusercontent.com/AleXu224/zzz_packet_capture/master/assets";
const NANOKA: &str = "https://static.nanoka.cc";
const USER_AGENT: &str = concat!("hollow-archive/", env!("CARGO_PKG_VERSION"));

/// Everything the pipeline and exporter need, plus where it came from.
pub struct DataSet {
    pub version: String,
    pub datamine: Datamine,
    pub schema_json: String,
    pub gamedata: GameData,
    pub from_cache: bool,
}

impl DataSet {
    pub fn schema(&self) -> Result<Schema> {
        Ok(Schema::from_json(&self.schema_json)?)
    }
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
}

/// `%LOCALAPPDATA%\hollow-archive\<sub>` (or the platform equivalent).
pub fn app_dir(sub: &str) -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.data_local_dir().join("hollow-archive").join(sub))
}

pub fn cache_dir() -> Option<PathBuf> {
    app_dir("data")
}

/// Compare dotted version strings numerically ("3.10" > "3.2").
pub fn version_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| {
        s.split('.')
            .map(|p| p.trim().parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(candidate) > parse(current)
}

fn vendored_set() -> DataSet {
    let version = serde_json::from_str::<Manifest>(vendored::MANIFEST)
        .map(|m| m.version)
        .unwrap_or_default();
    DataSet {
        version,
        datamine: Datamine::vendored(),
        schema_json: vendored::NAP.to_string(),
        gamedata: GameData::vendored(),
        from_cache: false,
    }
}

/// Vendored files, or the cached set if it is newer and complete.
pub fn load() -> DataSet {
    let vendored = vendored_set();
    let Some(dir) = cache_dir() else { return vendored };
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    let Some(manifest) = read("manifest.json").and_then(|s| serde_json::from_str::<Manifest>(&s).ok()) else {
        return vendored;
    };
    if !version_newer(&manifest.version, &vendored.version) {
        return vendored;
    }
    let cached = (|| {
        Some(DataSet {
            version: manifest.version.clone(),
            datamine: Datamine::from_json(&read("datamine.json")?).ok()?,
            schema_json: read("nap.json")?,
            gamedata: GameData::from_json(&read("nanoka.json")?).ok()?,
            from_cache: true,
        })
    })();
    match cached {
        Some(c) => {
            tracing::info!("using cached data files for {}", c.version);
            c
        }
        None => {
            tracing::warn!("cached data files incomplete; using vendored {}", vendored.version);
            vendored
        }
    }
}

fn get(url: &str) -> Result<String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(USER_AGENT)
        .build()
        .new_agent();
    agent
        .get(url)
        .call()
        .with_context(|| url.to_string())?
        .body_mut()
        .read_to_string()
        .with_context(|| url.to_string())
}

/// Result of a background check.
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    Available(String),
    Downloading(String),
    Installed(String),
    Failed(String),
}

/// Ask upstream which game version its data files are for.
pub fn check(current: &str) -> Result<UpdateStatus> {
    let m: Manifest = serde_json::from_str(&get(&format!("{UPSTREAM}/manifest.json"))?)?;
    Ok(if version_newer(&m.version, current) {
        UpdateStatus::Available(m.version)
    } else {
        UpdateStatus::UpToDate
    })
}

/// Download everything for the newest upstream version into the cache.
pub fn install() -> Result<String> {
    let dir = cache_dir().ok_or_else(|| anyhow!("no local data directory"))?;
    std::fs::create_dir_all(&dir)?;
    let manifest_json = get(&format!("{UPSTREAM}/manifest.json"))?;
    let manifest: Manifest = serde_json::from_str(&manifest_json)?;
    let datamine = get(&format!("{UPSTREAM}/datamine.json"))?;
    Datamine::from_json(&datamine).context("upstream datamine.json")?;
    let nap = get(&format!("{UPSTREAM}/nap.json"))?;
    Schema::from_json(&nap).context("upstream nap.json")?;
    let nanoka = fetch_nanoka()?;

    // Write the manifest last so a partial download never looks complete.
    std::fs::write(dir.join("datamine.json"), datamine)?;
    std::fs::write(dir.join("nap.json"), nap)?;
    std::fs::write(dir.join("nanoka.json"), nanoka)?;
    std::fs::write(dir.join("manifest.json"), manifest_json)?;
    Ok(manifest.version)
}

/// Same slimming as scripts/update-gamedata.py.
fn fetch_nanoka() -> Result<String> {
    #[derive(Deserialize)]
    struct NManifest {
        zzz: NZzz,
    }
    #[derive(Deserialize)]
    struct NZzz {
        live: String,
    }
    #[derive(Deserialize)]
    struct NChar {
        en: String,
        rank: u32,
    }
    #[derive(Deserialize)]
    struct NEquip {
        en: NEquipEn,
    }
    #[derive(Deserialize)]
    struct NEquipEn {
        name: String,
    }

    let ver = serde_json::from_str::<NManifest>(&get(&format!("{NANOKA}/manifest.json"))?)?
        .zzz
        .live;
    let chars: BTreeMap<u32, NChar> = serde_json::from_str(&get(&format!("{NANOKA}/zzz/{ver}/character.json"))?)?;
    let equip: BTreeMap<u32, NEquip> = serde_json::from_str(&get(&format!("{NANOKA}/zzz/{ver}/equipment.json"))?)?;
    let weapons: BTreeMap<u32, NChar> = serde_json::from_str(&get(&format!("{NANOKA}/zzz/{ver}/weapon.json"))?)?;
    let gd = GameData {
        version: ver,
        characters: chars
            .into_iter()
            .map(|(k, v)| (k, hollow_proto::gamedata::Named { en: v.en, rank: v.rank }))
            .collect(),
        equipment: equip
            .into_iter()
            .map(|(k, v)| (k, hollow_proto::gamedata::SetName { en: v.en.name }))
            .collect(),
        weapons: weapons
            .into_iter()
            .map(|(k, v)| (k, hollow_proto::gamedata::Named { en: v.en, rank: v.rank }))
            .collect(),
    };
    Ok(serde_json::to_string(&gd)?)
}

/// Fire-and-forget update check; the UI polls the receiver.
pub fn check_in_background(current: String) -> Receiver<UpdateStatus> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let status = match check(&current) {
            Ok(s) => s,
            Err(e) => UpdateStatus::Failed(format!("{e:#}")),
        };
        let _ = tx.send(status);
    });
    rx
}

pub fn install_in_background() -> Receiver<UpdateStatus> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let status = match install() {
            Ok(v) => UpdateStatus::Installed(v),
            Err(e) => UpdateStatus::Failed(format!("{e:#}")),
        };
        let _ = tx.send(status);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(version_newer("3.3", "3.2"));
        assert!(version_newer("3.10", "3.2"));
        assert!(version_newer("4.0", "3.9"));
        assert!(!version_newer("3.2", "3.2"));
        assert!(!version_newer("3.1", "3.2"));
        assert!(!version_newer("", "3.2"));
    }

    /// Hits GitHub + nanoka.cc; run with `cargo test -p hollow-archive -- --ignored`.
    #[test]
    #[ignore]
    fn install_from_upstream() {
        let v = install().unwrap();
        assert!(!v.is_empty());
        let dir = cache_dir().unwrap();
        for f in ["manifest.json", "datamine.json", "nap.json", "nanoka.json"] {
            assert!(dir.join(f).exists(), "{f}");
        }
        let gd = GameData::from_json(&std::fs::read_to_string(dir.join("nanoka.json")).unwrap()).unwrap();
        assert!(gd.agent_name(1011).is_some());
        assert!(matches!(check(&v).unwrap(), UpdateStatus::UpToDate));
    }

    #[test]
    fn vendored_loads() {
        let d = vendored_set();
        assert_eq!(d.version, "3.2");
        assert!(d.schema().unwrap().len() > 1000);
    }
}
