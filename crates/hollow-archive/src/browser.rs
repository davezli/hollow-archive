//! Opening a web page from an elevated process.
//!
//! The app runs as administrator (pktmon needs it), so a browser launched
//! straight from here would run elevated too. `explorer.exe` hands the URL to
//! the user's existing, unelevated shell, which opens it in their default
//! browser with their normal rights. explorer exits non-zero even on success,
//! so it is spawned and never waited on.

#[cfg(windows)]
pub fn open(url: &str) -> std::io::Result<()> {
    std::process::Command::new("explorer.exe").arg(url).spawn().map(|_| ())
}

#[cfg(not(windows))]
pub fn open(_url: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Windows only"))
}
