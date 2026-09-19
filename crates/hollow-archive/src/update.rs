//! Self-update against GitHub releases (`.github/workflows/release.yml` publishes
//! `hollow-archive-<ver>-<target>.zip` containing the exe).

use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result};

const OWNER: &str = "davezli";
const REPO: &str = "hollow-archive";
const BIN: &str = "hollow-archive";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub enum AppUpdate {
    UpToDate,
    Available(String),
    Installing(String),
    /// New exe is in place; takes effect on restart.
    Installed(String),
    Failed(String),
}

fn configured() -> Result<Box<dyn self_update::update::ReleaseUpdate>> {
    self_update::backends::github::Update::configure()
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name(BIN)
        .target(self_update::get_target())
        .bin_path_in_archive(&format!("{BIN}.exe"))
        .current_version(CURRENT)
        .no_confirm(true)
        .show_output(false)
        .show_download_progress(false)
        .build()
        .context("configuring updater")
}

pub fn check() -> Result<AppUpdate> {
    let latest = configured()?.get_latest_release().context("fetching latest release")?;
    let newer = self_update::version::bump_is_greater(CURRENT, &latest.version).unwrap_or(false);
    Ok(if newer {
        AppUpdate::Available(latest.version)
    } else {
        AppUpdate::UpToDate
    })
}

pub fn install() -> Result<String> {
    let status = configured()?
        .update()
        .context("downloading and replacing the executable")?;
    Ok(status.version().to_string())
}

pub fn check_in_background() -> Receiver<AppUpdate> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(match check() {
            Ok(s) => s,
            Err(e) => AppUpdate::Failed(format!("{e:#}")),
        });
    });
    rx
}

pub fn install_in_background() -> Receiver<AppUpdate> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(match install() {
            Ok(v) => AppUpdate::Installed(v),
            Err(e) => AppUpdate::Failed(format!("{e:#}")),
        });
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits api.github.com; run with `cargo test -p hollow-archive -- --ignored`.
    #[test]
    #[ignore]
    fn latest_release_resolves() {
        let latest = configured().unwrap().get_latest_release().unwrap();
        assert!(!latest.version.is_empty());
        let target = self_update::get_target();
        assert!(
            latest.assets.iter().any(|a| a.name.contains(target)),
            "no asset for {target}: {:?}",
            latest.assets
        );
        assert!(matches!(
            check().unwrap(),
            AppUpdate::UpToDate | AppUpdate::Available(_)
        ));
    }
}
