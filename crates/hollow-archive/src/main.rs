//! Hollow Archive — headless CLI (M2). The egui UI lands in M4.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hollow_proto::export::{export, ExportSettings};
use hollow_proto::gamedata::GameData;
use hollow_proto::model::PlayerData;
use hollow_proto::{fixture, Event, Pipeline, SessionState};

#[derive(Parser, Debug)]
#[command(name = "hollow-archive", version, about)]
struct Cli {
    /// Game region: Europe, America, Asia, or "TW,HK,MO".
    #[arg(long, default_value = "America")]
    region: String,

    /// Replay a recording (.pcapng from pktmon/Wireshark, or zzz_packet_capture's .json)
    /// instead of capturing live.
    #[arg(long)]
    fixture: Option<PathBuf>,

    /// Write the Zenless Optimizer JSON here ("-" for stdout).
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Print every pipeline event as it happens.
    #[arg(long, short)]
    verbose: bool,

    /// Pad every disc to four substats with empty keys, like zzz_packet_capture.
    #[arg(long)]
    pad_substats: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(path) = &cli.fixture else {
        bail!("live capture is not implemented yet; pass --fixture <file.pcapng>");
    };

    let packets = fixture::read(path).with_context(|| format!("reading {}", path.display()))?;
    eprintln!("{}: {} game datagrams", path.display(), packets.len());

    let mut pipeline = Pipeline::for_region(&cli.region)?;
    let mut data = PlayerData::default();
    let mut warnings = 0usize;
    for p in &packets {
        for ev in pipeline.feed(&p.payload, p.direction, p.unix_secs()) {
            match &ev {
                Event::ServerKeyFound => eprintln!("[t={}] server_rand_key found", p.unix_secs()),
                Event::SessionEstablished { clock_delta } => {
                    eprintln!(
                        "[t={}] session established (clock delta {clock_delta:+}s)",
                        p.unix_secs()
                    )
                }
                Event::Agents(v) => {
                    eprintln!("[t={}] {} agents", p.unix_secs(), v.len());
                    data.agents.extend(v.iter().cloned());
                }
                Event::WEngines(v) => {
                    eprintln!("[t={}] {} w-engines", p.unix_secs(), v.len());
                    data.wengines.extend(v.iter().cloned());
                }
                Event::Discs(v) => {
                    eprintln!("[t={}] {} drive discs", p.unix_secs(), v.len());
                    data.discs.extend(v.iter().cloned());
                }
                Event::UnhandledCommand { cmd_id, len } => {
                    if cli.verbose {
                        eprintln!("[t={}] cmd {cmd_id} ({len} bytes)", p.unix_secs());
                    }
                }
                Event::Warning(w) => {
                    warnings += 1;
                    if cli.verbose || warnings <= 5 {
                        eprintln!("[t={}] warning: {w}", p.unix_secs());
                    }
                }
            }
        }
    }

    let stats = pipeline.stats();
    eprintln!(
        "done: state={:?} datagrams={} messages={} undecodable={} kcp_gaps={} warnings={}",
        pipeline.state(),
        stats.datagrams,
        stats.messages,
        stats.undecodable,
        stats.kcp_gaps,
        warnings
    );
    eprintln!(
        "captured: {} agents, {} w-engines, {} discs",
        data.agents.len(),
        data.wengines.len(),
        data.discs.len()
    );

    if pipeline.state() != SessionState::Established {
        bail!("session key was never derived — wrong region, or the recording does not include the login handshake");
    }

    if let Some(out) = &cli.output {
        let settings = ExportSettings {
            pad_substats: cli.pad_substats,
            ..Default::default()
        };
        let zod = export(&data, &GameData::vendored(), &settings);
        let json = serde_json::to_string_pretty(&zod)?;
        if out.as_os_str() == "-" {
            println!("{json}");
        } else {
            std::fs::write(out, json).with_context(|| format!("writing {}", out.display()))?;
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
