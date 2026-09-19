//! Hollow Archive — Zenless Zone Zero inventory exporter.
//! GUI by default; `--headless` for the CLI used in development and bug reports.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod admin;
mod app;
mod capture;
mod datafiles;
mod theme;
mod update;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hollow_proto::export::{export, ExportSettings};
use hollow_proto::{Event, SessionState};

use crate::capture::{Accumulator, Backend, CaptureConfig, CaptureEvent, CaptureSession, Source};

#[derive(Parser, Debug)]
#[command(name = "hollow-archive", version, about)]
struct Cli {
    /// Game region: Europe, America, Asia, or "TW,HK,MO". Remembered by the GUI.
    #[arg(long)]
    region: Option<String>,

    /// Packet capture backend (pktmon needs no driver; pcap needs Npcap and a `--features pcap` build).
    #[arg(long = "capture-backend", value_enum)]
    capture_backend: Option<Backend>,

    /// Replay a recording (.pcapng from pktmon/Wireshark, or zzz_packet_capture's .json)
    /// instead of capturing live.
    #[arg(long)]
    fixture: Option<PathBuf>,

    /// Save every game datagram seen during the session to this pcapng (for bug reports).
    #[arg(long)]
    record: Option<PathBuf>,

    /// Run without the GUI; print progress to stderr and exit when done.
    #[arg(long)]
    headless: bool,

    /// Headless: write the Zenless Optimizer JSON here ("-" for stdout).
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Headless: print every pipeline event.
    #[arg(long, short)]
    verbose: bool,

    /// Pad every disc to four substats with empty keys, like zzz_packet_capture.
    #[arg(long)]
    pad_substats: bool,

    /// Start capturing as soon as the window opens.
    #[arg(long)]
    autostart: bool,

    /// Do not try to relaunch as administrator.
    #[arg(long)]
    no_admin: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("hollow_archive=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    let needs_capture = cli.fixture.is_none();
    if needs_capture && !cli.no_admin && !admin::is_elevated() {
        if let Err(e) = admin::relaunch_elevated() {
            tracing::warn!("could not relaunch elevated: {e:#}");
        }
    }

    if cli.headless {
        headless(cli)
    } else {
        gui(cli)
    }
}

fn gui(cli: Cli) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 520.0])
            .with_min_inner_size([760.0, 480.0])
            .with_decorations(false)
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("valid icon.png"))
            .with_title("Hollow Archive"),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Hollow Archive",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                cc,
                cli.region,
                cli.capture_backend,
                cli.fixture,
                cli.record,
                cli.autostart,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

fn headless(cli: Cli) -> Result<()> {
    let region = cli.region.unwrap_or_else(|| "America".into());
    let source = match &cli.fixture {
        Some(p) => Source::Fixture(p.clone()),
        None => Source::Live(cli.capture_backend.unwrap_or(Backend::Pktmon)),
    };
    let data = std::sync::Arc::new(datafiles::load());
    eprintln!("region {region}, source {source:?}, data files {}", data.version);
    let gamedata = data.gamedata.clone();
    let session = CaptureSession::start(CaptureConfig {
        region,
        source,
        record_to: cli.record,
        data,
    });
    let mut acc = Accumulator::default();
    let mut state = SessionState::Initial;
    let mut warnings = 0usize;

    for ev in session.events.iter() {
        match ev {
            CaptureEvent::Listening => eprintln!("listening on UDP {}", hollow_proto::GAME_PORT),
            CaptureEvent::GameTrafficSeen => eprintln!("game traffic detected"),
            CaptureEvent::Progress { datagrams } => {
                if cli.verbose {
                    eprintln!("{datagrams} datagrams");
                }
            }
            CaptureEvent::Pipeline(ev) => match &ev {
                Event::ServerKeyFound => {
                    state = SessionState::HaveServerKey;
                    eprintln!("server_rand_key found");
                }
                Event::SessionEstablished { clock_delta } => {
                    state = SessionState::Established;
                    eprintln!("session established (clock delta {clock_delta:+}s)");
                }
                Event::UnhandledCommand { cmd_id, len } => {
                    if cli.verbose {
                        eprintln!("cmd {cmd_id} ({len} bytes)");
                    }
                }
                Event::Warning(w) => {
                    warnings += 1;
                    if cli.verbose || warnings <= 5 {
                        eprintln!("warning: {w}");
                    }
                }
                data => {
                    acc.absorb(data);
                    let d = &acc.data;
                    eprintln!(
                        "inventory: {} agents, {} w-engines, {} discs",
                        d.agents.len(),
                        d.wengines.len(),
                        d.discs.len()
                    );
                    if acc.complete() {
                        session.request_stop();
                    }
                }
            },
            CaptureEvent::Recorded(p) => eprintln!("recording written to {}", p.display()),
            CaptureEvent::Error(e) => eprintln!("error: {e}"),
            CaptureEvent::Stopped => break,
        }
    }

    eprintln!("done: state={state:?} warnings={warnings}");
    if state != SessionState::Established {
        bail!("session key was never derived — wrong region, or the login handshake was not observed");
    }
    if !acc.complete() {
        eprintln!("warning: partial capture (missing agents, w-engines or discs)");
    }

    if let Some(out) = &cli.output {
        let settings = ExportSettings {
            pad_substats: cli.pad_substats,
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&export(&acc.data, &gamedata, &settings))?;
        if out.as_os_str() == "-" {
            println!("{json}");
        } else {
            std::fs::write(out, json).with_context(|| format!("writing {}", out.display()))?;
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
