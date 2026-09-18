//! The egui shell. One window, four sections: status, captured counts, export, log.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Align, Layout, RichText};
use hollow_proto::export::{export, ExportSettings};
use hollow_proto::gamedata::GameData;
use hollow_proto::proto::datamine::Datamine;
use hollow_proto::Event;
use serde::{Deserialize, Serialize};

use crate::capture::{Accumulator, CaptureConfig, CaptureEvent, CaptureSession, Source};
use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    WaitingForGame,
    Handshake,
    Capturing,
    Done,
    Error(String),
}

impl Phase {
    fn label(&self) -> &str {
        match self {
            Phase::Idle => "IDLE",
            Phase::Starting => "STARTING",
            Phase::WaitingForGame => "WAITING FOR GAME",
            Phase::Handshake => "HANDSHAKE",
            Phase::Capturing => "CAPTURING",
            Phase::Done => "DONE",
            Phase::Error(_) => "ERROR",
        }
    }

    fn hint(&self) -> String {
        match self {
            Phase::Idle => "Press Start, then launch Zenless Zone Zero and log in.".into(),
            Phase::Starting => "Opening packet capture…".into(),
            Phase::WaitingForGame => {
                "Listening. Launch Zenless Zone Zero and log in.\nAlready in game? Log out to the title screen and back in.".into()
            }
            Phase::Handshake => "Game traffic seen — deriving the session key…".into(),
            Phase::Capturing => "Session decrypted. Inventory arrives as the game loads.".into(),
            Phase::Done => "Everything captured. Export below.".into(),
            Phase::Error(e) => e.clone(),
        }
    }

    fn color(&self) -> egui::Color32 {
        match self {
            Phase::Idle | Phase::Starting => theme::MUTED,
            Phase::WaitingForGame | Phase::Handshake => theme::AMBER,
            Phase::Capturing | Phase::Done => theme::TEAL,
            Phase::Error(_) => theme::RED,
        }
    }

    fn busy(&self) -> bool {
        matches!(
            self,
            Phase::Starting | Phase::WaitingForGame | Phase::Handshake | Phase::Capturing
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub region: String,
    pub export: ExportSettings,
    pub show_log: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            region: "America".into(),
            export: ExportSettings::default(),
            show_log: false,
        }
    }
}

pub struct App {
    settings: Settings,
    regions: Vec<String>,
    gamedata: GameData,
    phase: Phase,
    session: Option<CaptureSession>,
    acc: Accumulator,
    datagrams: u64,
    log: Vec<String>,
    toast: Option<(String, f64)>,
    fixture: Option<PathBuf>,
    record_to: Option<PathBuf>,
    started_capturing_at: Option<f64>,
}

const SETTINGS_KEY: &str = "settings";

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        region: Option<String>,
        fixture: Option<PathBuf>,
        record_to: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        if let Some(r) = region {
            settings.region = r;
        }
        let dm = Datamine::vendored();
        let regions: Vec<String> = dm.regions().map(str::to_string).collect();
        if dm.region_seed(&settings.region).is_err() {
            settings.region = regions.first().cloned().unwrap_or_default();
        }
        Self {
            settings,
            regions,
            gamedata: GameData::vendored(),
            phase: Phase::Idle,
            session: None,
            acc: Accumulator::default(),
            datagrams: 0,
            log: Vec::new(),
            toast: None,
            fixture,
            record_to,
            started_capturing_at: None,
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        tracing::info!("{line}");
        self.log.push(line);
        if self.log.len() > 500 {
            self.log.drain(..100);
        }
    }

    fn start(&mut self) {
        self.acc = Accumulator::default();
        self.datagrams = 0;
        self.started_capturing_at = None;
        let source = match &self.fixture {
            Some(p) => Source::Fixture(p.clone()),
            #[cfg(windows)]
            None => Source::Pktmon,
            #[cfg(not(windows))]
            None => {
                self.phase = Phase::Error("Live capture is only supported on Windows. Use --fixture.".into());
                return;
            }
        };
        let config = CaptureConfig {
            region: self.settings.region.clone(),
            source,
            record_to: self.record_to.clone(),
        };
        self.log(format!(
            "starting capture (region {}, {:?})",
            config.region, config.source
        ));
        self.session = Some(CaptureSession::start(config));
        self.phase = Phase::Starting;
    }

    fn stop(&mut self) {
        if let Some(s) = &self.session {
            s.request_stop();
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        let Some(session) = &self.session else { return };
        let mut events = Vec::new();
        while let Ok(ev) = session.events.try_recv() {
            events.push(ev);
        }
        let finished = session.is_finished();
        for ev in events {
            self.handle(ev, ctx);
        }
        if finished {
            self.session = None;
            if self.phase.busy() {
                self.phase = if self.acc.complete() { Phase::Done } else { Phase::Idle };
            }
        }
        if self.session.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn handle(&mut self, ev: CaptureEvent, ctx: &egui::Context) {
        match ev {
            CaptureEvent::Listening => {
                self.phase = Phase::WaitingForGame;
                self.log("listening on UDP 20501");
            }
            CaptureEvent::GameTrafficSeen => {
                if self.phase == Phase::WaitingForGame {
                    self.phase = Phase::Handshake;
                }
                self.log("game traffic detected");
            }
            CaptureEvent::Progress { datagrams } => self.datagrams = datagrams,
            CaptureEvent::Pipeline(ev) => match ev {
                Event::ServerKeyFound => self.log("server key extracted from login token"),
                Event::SessionEstablished { clock_delta } => {
                    self.phase = Phase::Capturing;
                    self.started_capturing_at = Some(ctx.input(|i| i.time));
                    self.log(format!("session key derived (clock delta {clock_delta:+}s)"));
                }
                Event::Warning(w) => self.log(format!("warning: {w}")),
                Event::UnhandledCommand { .. } => {}
                ref data => {
                    if self.acc.absorb(data) {
                        let d = &self.acc.data;
                        self.log(format!(
                            "inventory: {} agents, {} w-engines, {} discs",
                            d.agents.len(),
                            d.wengines.len(),
                            d.discs.len()
                        ));
                        if self.acc.complete() {
                            self.phase = Phase::Done;
                            self.stop();
                        }
                    }
                }
            },
            CaptureEvent::Recorded(p) => self.log(format!("capture saved to {}", p.display())),
            CaptureEvent::Error(e) => {
                self.log(format!("error: {e}"));
                self.phase = Phase::Error(e);
            }
            CaptureEvent::Stopped => {
                self.log("capture stopped");
                if self.phase.busy() {
                    // Handshake never completed within the run: say why it probably failed.
                    self.phase = match self.phase {
                        Phase::Handshake => Phase::Error(
                            "Saw game traffic but could not decrypt it. Wrong region selected, or the game was already past the login screen — log out and back in.".into(),
                        ),
                        _ => Phase::Idle,
                    };
                }
            }
        }
    }

    fn export_json(&self) -> String {
        let zod = export(&self.acc.data, &self.gamedata, &self.settings.export);
        serde_json::to_string_pretty(&zod).unwrap_or_default()
    }

    fn toast(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time)));
    }

    // ---- UI ---------------------------------------------------------------

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("HOLLOW ARCHIVE").heading().color(theme::TEAL).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(theme::MUTED),
                );
            });
        });
        ui.label(
            RichText::new(format!(
                "Inter-Knot field terminal · game data {}",
                self.gamedata.version
            ))
            .small()
            .color(theme::MUTED),
        );
        ui.add_space(6.0);
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        let phase = self.phase.clone();
        let mut start = false;
        let mut stop = false;
        theme::section(ui, "STATUS", |ui| {
            ui.horizontal(|ui| {
                // Status marker: painted (the default font has no ● glyph); blinks while working.
                let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                let lit = !phase.busy() || (ui.input(|i| i.time) * 2.0).fract() < 0.5;
                let c = rect.center();
                if lit {
                    ui.painter().circle_filled(c, 6.0, phase.color());
                } else {
                    ui.painter()
                        .circle_stroke(c, 6.0, egui::Stroke::new(1.5_f32, phase.color()));
                }
                ui.label(RichText::new(phase.label()).size(18.0).strong().color(phase.color()));
                if phase.busy() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} pkts", self.datagrams))
                                .monospace()
                                .small()
                                .color(theme::MUTED),
                        );
                    });
                }
            });
            ui.label(RichText::new(phase.hint()).color(if matches!(phase, Phase::Error(_)) {
                theme::TEXT
            } else {
                theme::MUTED
            }));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!phase.busy(), |ui| {
                    ui.label(RichText::new("Region").color(theme::MUTED));
                    egui::ComboBox::from_id_salt("region")
                        .selected_text(&self.settings.region)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for r in &self.regions {
                                ui.selectable_value(&mut self.settings.region, r.clone(), r);
                            }
                        });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if phase.busy() {
                        if ui.button("Stop").clicked() {
                            stop = true;
                        }
                    } else {
                        let label = if self.acc.complete() {
                            "Capture again"
                        } else {
                            "Start capture"
                        };
                        if theme::primary(ui, label, true).clicked() {
                            start = true;
                        }
                    }
                });
            });
        });
        if start {
            self.start();
        }
        if stop {
            self.stop();
        }
        if phase.busy() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    fn counts(&self, ui: &mut egui::Ui) {
        theme::section(ui, "CAPTURED", |ui| {
            let d = &self.acc.data;
            let tiles = [
                ("Agents", d.agents.len()),
                ("W-Engines", d.wengines.len()),
                ("Drive Discs", d.discs.len()),
            ];
            ui.columns(3, |cols| {
                for (col, (name, n)) in cols.iter_mut().zip(tiles) {
                    col.vertical_centered(|ui| {
                        let color = if n > 0 { theme::TEXT } else { theme::MUTED };
                        ui.label(RichText::new(n.to_string()).size(26.0).monospace().color(color));
                        ui.label(RichText::new(name).small().color(theme::MUTED));
                    });
                }
            });
        });
    }

    fn export_panel(&mut self, ui: &mut egui::Ui) {
        let have_data =
            !self.acc.data.agents.is_empty() || !self.acc.data.wengines.is_empty() || !self.acc.data.discs.is_empty();
        let mut copy = false;
        let mut save = false;
        theme::section(ui, "EXPORT · ZENLESS OPTIMIZER", |ui| {
            let s = &mut self.settings.export;
            egui::Grid::new("export-grid")
                .num_columns(4)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("").small());
                    ui.label(RichText::new("min level").small().color(theme::MUTED));
                    ui.label(RichText::new("min rarity").small().color(theme::MUTED));
                    ui.end_row();

                    row(
                        ui,
                        "Agents",
                        &mut s.export_agents,
                        &mut s.min_agent_level,
                        &mut s.min_agent_rarity,
                        60,
                        4,
                    );
                    row(
                        ui,
                        "W-Engines",
                        &mut s.export_wengines,
                        &mut s.min_wengine_level,
                        &mut s.min_wengine_rarity,
                        60,
                        3,
                    );
                    row(
                        ui,
                        "Drive Discs",
                        &mut s.export_discs,
                        &mut s.min_disc_level,
                        &mut s.min_disc_rarity,
                        15,
                        3,
                    );
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary(ui, "Copy to clipboard", have_data).clicked() {
                    copy = true;
                }
                if ui.add_enabled(have_data, egui::Button::new("Save file…")).clicked() {
                    save = true;
                }
            });
        });
        if copy {
            let json = self.export_json();
            ui.ctx().copy_text(json);
            self.toast(ui.ctx(), "Copied — paste into Zenless Optimizer's import.");
        }
        if save {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("zenless-optimizer.json")
                .add_filter("JSON", &["json"])
                .save_file()
            {
                match std::fs::write(&path, self.export_json()) {
                    Ok(()) => self.toast(ui.ctx(), format!("Saved {}", path.display())),
                    Err(e) => self.toast(ui.ctx(), format!("Save failed: {e}")),
                }
            }
        }
    }

    fn log_panel(&mut self, ui: &mut egui::Ui) {
        let header = format!("Log ({})", self.log.len());
        let resp = egui::CollapsingHeader::new(RichText::new(header).small().color(theme::MUTED))
            .default_open(self.settings.show_log)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            let color = if line.starts_with("error") {
                                theme::RED
                            } else if line.starts_with("warning") {
                                theme::AMBER
                            } else {
                                theme::MUTED
                            };
                            ui.label(RichText::new(line).monospace().small().color(color));
                        }
                    });
            });
        self.settings.show_log = resp.fully_open();
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        let age = ctx.input(|i| i.time) - at;
        if age > 3.5 {
            self.toast = None;
            return;
        }
        egui::Area::new("toast".into())
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -16.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme::PANEL_RAISED)
                    .stroke(egui::Stroke::new(1.0_f32, theme::TEAL))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).color(theme::TEXT));
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn row(
    ui: &mut egui::Ui,
    name: &str,
    on: &mut bool,
    min_level: &mut u32,
    min_rarity: &mut u32,
    max_level: u32,
    min_r: u32,
) {
    ui.checkbox(on, name);
    ui.add_enabled(*on, egui::DragValue::new(min_level).range(0..=max_level));
    ui.add_enabled_ui(*on, |ui| {
        egui::ComboBox::from_id_salt(name)
            .selected_text(rarity_name(*min_rarity))
            .width(52.0)
            .show_ui(ui, |ui| {
                for r in min_r..=5 {
                    ui.selectable_value(min_rarity, r, rarity_name(r));
                }
            });
    });
    ui.label("");
    ui.end_row();
}

fn rarity_name(r: u32) -> &'static str {
    match r {
        3 => "B",
        4 => "A",
        5 => "S",
        _ => "?",
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(14))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.header(ui);
                    self.status(ui);
                    ui.add_space(8.0);
                    self.counts(ui);
                    ui.add_space(8.0);
                    self.export_panel(ui);
                    ui.add_space(8.0);
                    self.log_panel(ui);
                });
            });
        self.toast_ui(ctx);
    }
}
