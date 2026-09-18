//! Capture session: a background thread that owns the packet source (live
//! pktmon, or a recording) and a `Pipeline`, and reports over a channel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use hollow_proto::fixture::{self, Packet};
use hollow_proto::model::PlayerData;
use hollow_proto::{Direction, Event, Pipeline};

/// What the capture thread tells the UI.
#[derive(Debug)]
pub enum CaptureEvent {
    /// Packet source is open and listening.
    Listening,
    /// First game datagram seen.
    GameTrafficSeen,
    Pipeline(Event),
    /// Periodic counter update.
    Progress {
        datagrams: u64,
    },
    /// Recording written (when `--record` / "Save capture" was requested).
    Recorded(PathBuf),
    Error(String),
    /// Thread finished (after stop, fixture end, or error).
    Stopped,
}

#[derive(Debug, Clone)]
pub enum Source {
    #[cfg(windows)]
    Pktmon,
    Fixture(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub region: String,
    pub source: Source,
    /// Write every game datagram seen to this pcapng when the session ends.
    pub record_to: Option<PathBuf>,
}

pub struct CaptureSession {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub events: Receiver<CaptureEvent>,
}

impl CaptureSession {
    pub fn start(config: CaptureConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                if let Err(e) = run(config, &tx, &stop2) {
                    let _ = tx.send(CaptureEvent::Error(format!("{e:#}")));
                }
                let _ = tx.send(CaptureEvent::Stopped);
            })
            .expect("spawn capture thread");
        Self {
            stop,
            thread: Some(thread),
            events: rx,
        }
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(config: CaptureConfig, tx: &Sender<CaptureEvent>, stop: &AtomicBool) -> Result<()> {
    let mut pipeline = Pipeline::for_region(&config.region)?;
    let mut recorded: Vec<Packet> = Vec::new();
    let mut datagrams = 0u64;
    let mut seen_game = false;

    let mut on_packet = |payload: &[u8], dir: Direction, ts: Duration| -> bool {
        datagrams += 1;
        if !seen_game {
            seen_game = true;
            let _ = tx.send(CaptureEvent::GameTrafficSeen);
        }
        if config.record_to.is_some() {
            recorded.push(Packet {
                timestamp: ts,
                direction: dir,
                payload: payload.to_vec(),
            });
        }
        for ev in pipeline.feed(payload, dir, ts.as_secs() as i64) {
            if tx.send(CaptureEvent::Pipeline(ev)).is_err() {
                return false;
            }
        }
        if datagrams.is_multiple_of(50) {
            let _ = tx.send(CaptureEvent::Progress { datagrams });
        }
        true
    };

    match &config.source {
        Source::Fixture(path) => {
            let packets = fixture::read(path).with_context(|| format!("reading {}", path.display()))?;
            let _ = tx.send(CaptureEvent::Listening);
            for p in &packets {
                if stop.load(Ordering::Relaxed) || !on_packet(&p.payload, p.direction, p.timestamp) {
                    break;
                }
            }
        }
        #[cfg(windows)]
        Source::Pktmon => {
            live_pktmon(tx, stop, &mut on_packet)?;
        }
    }

    let _ = tx.send(CaptureEvent::Progress { datagrams });
    if let Some(path) = &config.record_to {
        fixture::write_pcapng(path, &recorded).with_context(|| format!("writing {}", path.display()))?;
        let _ = tx.send(CaptureEvent::Recorded(path.clone()));
    }
    Ok(())
}

#[cfg(windows)]
fn live_pktmon(
    tx: &Sender<CaptureEvent>,
    stop: &AtomicBool,
    on_packet: &mut dyn FnMut(&[u8], Direction, Duration) -> bool,
) -> Result<()> {
    use hollow_proto::frame;
    use pktmon::filter::{PktMonFilter, TransportProtocol};
    use pktmon::{Capture, PacketPayload};

    let mut capture =
        Capture::new().map_err(|e| anyhow!("opening pktmon: {e} (is Hollow Archive running as administrator?)"))?;
    capture
        .add_filter(PktMonFilter {
            name: "Hollow Archive ZZZ".into(),
            transport_protocol: Some(TransportProtocol::UDP),
            port: hollow_proto::GAME_PORT.into(),
            ..PktMonFilter::default()
        })
        .map_err(|e| anyhow!("adding pktmon filter: {e}"))?;
    capture.start().map_err(|e| anyhow!("starting pktmon capture: {e}"))?;
    let _ = tx.send(CaptureEvent::Listening);

    // pktmon reports the same packet once per NDIS component it crosses; keep one.
    let mut component: Option<u16> = None;
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Relaxed) {
            let pkt = match capture.next_packet_timeout(Duration::from_millis(200)) {
                Ok(p) => p,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("pktmon capture closed"),
            };
            if *component.get_or_insert(pkt.component_id) != pkt.component_id {
                continue;
            }
            let udp = match &pkt.payload {
                PacketPayload::Ethernet(b) => frame::ethernet(b),
                PacketPayload::IP(b) => frame::ip(b),
                PacketPayload::UDP(b) => frame::udp(b),
                PacketPayload::Unknown(b) => frame::ethernet(b).or_else(|| frame::ip(b)),
                _ => None,
            };
            let Some(udp) = udp else { continue };
            let Some(dir) = Direction::from_ports(udp.src_port, udp.dst_port) else {
                continue;
            };
            let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
            if !on_packet(udp.payload, dir, ts) {
                break;
            }
        }
        Ok(())
    })();

    let _ = capture.stop();
    let _ = capture.unload();
    result
}

/// Accumulates pipeline data events into player data (shared by CLI and UI).
#[derive(Default)]
pub struct Accumulator {
    pub data: PlayerData,
}

impl Accumulator {
    /// Returns true if the event carried inventory data.
    pub fn absorb(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Agents(v) => self.data.agents.extend(v.iter().cloned()),
            Event::WEngines(v) => self.data.wengines.extend(v.iter().cloned()),
            Event::Discs(v) => self.data.discs.extend(v.iter().cloned()),
            _ => return false,
        }
        true
    }

    pub fn complete(&self) -> bool {
        !self.data.agents.is_empty() && !self.data.wengines.is_empty() && !self.data.discs.is_empty()
    }
}
