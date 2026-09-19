//! The egui shell: a frameless window, hero art with the capture status on the
//! left, a compact panel of headings and icon buttons on the right.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use egui::{Align, Align2, Color32, FontFamily, FontId, Layout, Mesh, Pos2, Rect, RichText, Shape, UiBuilder, Vec2};
use egui_material_icons::icons as ic;
use hollow_proto::export::{export, ExportSettings};
use hollow_proto::Event;
use serde::{Deserialize, Serialize};

use crate::capture::{Accumulator, Backend, CaptureConfig, CaptureEvent, CaptureSession, Source};
use crate::datafiles::{self, DataSet, UpdateStatus};
use crate::theme;
use crate::update::{self, AppUpdate};

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
            Phase::Idle => "STANDBY",
            Phase::Starting => "STARTING",
            Phase::WaitingForGame => "WAITING FOR GAME",
            Phase::Handshake => "HANDSHAKE",
            Phase::Capturing => "CAPTURING",
            Phase::Done => "ARCHIVED",
            Phase::Error(_) => "ERROR",
        }
    }

    fn short(&self) -> &str {
        match self {
            Phase::Idle => "Idle",
            Phase::Starting => "Starting…",
            Phase::WaitingForGame => "Waiting for game",
            Phase::Handshake => "Handshake…",
            Phase::Capturing => "Capturing",
            Phase::Done => "Complete",
            Phase::Error(_) => "Error",
        }
    }

    fn hint(&self) -> String {
        match self {
            Phase::Idle => "Press ▶, then launch Zenless Zone Zero and log in.".into(),
            Phase::Starting => "Opening packet capture…".into(),
            Phase::WaitingForGame => {
                "Listening. Launch Zenless Zone Zero and log in.\nAlready in game? Log out to the title screen and back in.".into()
            }
            Phase::Handshake => "Game traffic seen — deriving the session key…".into(),
            Phase::Capturing => "Session decrypted. Inventory arrives as the game loads.".into(),
            Phase::Done => "Everything captured. Copy it into Zenless Optimizer.".into(),
            Phase::Error(e) => e.clone(),
        }
    }

    fn color(&self) -> Color32 {
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
#[serde(default)]
pub struct Settings {
    pub region: String,
    pub backend: Backend,
    pub save_captures: bool,
    pub export: ExportSettings,
    pub show_log: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            region: "America".into(),
            backend: Backend::Pktmon,
            save_captures: false,
            export: ExportSettings::default(),
            show_log: false,
        }
    }
}

pub struct App {
    settings: Settings,
    data: Arc<DataSet>,
    regions: Vec<String>,
    phase: Phase,
    session: Option<CaptureSession>,
    acc: Accumulator,
    datagrams: u64,
    log: Vec<String>,
    toast: Option<(String, f64)>,
    fixture: Option<PathBuf>,
    record_to: Option<PathBuf>,
    show_capture_settings: bool,
    show_export_settings: bool,
    update: UpdateStatus,
    update_rx: Option<Receiver<UpdateStatus>>,
    reload_data_when_idle: bool,
    app_update: AppUpdate,
    app_update_rx: Option<Receiver<AppUpdate>>,
}

const SETTINGS_KEY: &str = "settings";
const HERO_FRACTION: f32 = 0.54;

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        region: Option<String>,
        backend: Option<Backend>,
        fixture: Option<PathBuf>,
        record_to: Option<PathBuf>,
        autostart: bool,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();
        if let Some(r) = region {
            settings.region = r;
        }
        if let Some(b) = backend {
            settings.backend = b;
        }
        if !cfg!(feature = "pcap") {
            settings.backend = Backend::Pktmon;
        }
        let data = Arc::new(datafiles::load());
        let regions: Vec<String> = data.datamine.regions().map(str::to_string).collect();
        if data.datamine.region_seed(&settings.region).is_err() {
            settings.region = regions.first().cloned().unwrap_or_default();
        }
        let update_rx = Some(datafiles::check_in_background(data.version.clone()));
        let mut app = Self {
            settings,
            data,
            regions,
            phase: Phase::Idle,
            session: None,
            acc: Accumulator::default(),
            datagrams: 0,
            log: Vec::new(),
            toast: None,
            fixture,
            record_to,
            show_capture_settings: false,
            show_export_settings: false,
            update: UpdateStatus::UpToDate,
            update_rx,
            reload_data_when_idle: false,
            app_update: AppUpdate::UpToDate,
            app_update_rx: if cfg!(debug_assertions) {
                None
            } else {
                Some(update::check_in_background())
            },
        };
        app.log(format!(
            "data files {} ({})",
            app.data.version,
            if app.data.from_cache { "downloaded" } else { "built-in" }
        ));
        if autostart {
            app.start();
        }
        app
    }

    fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        tracing::info!("{line}");
        self.log.push(line);
        if self.log.len() > 500 {
            self.log.drain(..100);
        }
    }

    fn toast(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time)));
    }

    fn captures_dir() -> Option<PathBuf> {
        datafiles::app_dir("captures")
    }

    fn start(&mut self) {
        self.acc = Accumulator::default();
        self.datagrams = 0;
        let source = match &self.fixture {
            Some(p) => Source::Fixture(p.clone()),
            None => Source::Live(self.settings.backend),
        };
        let record_to = self.record_to.clone().or_else(|| {
            if !self.settings.save_captures {
                return None;
            }
            let dir = Self::captures_dir()?;
            std::fs::create_dir_all(&dir).ok()?;
            Some(dir.join(format!("zzz-{}.pcapng", chrono::Local::now().format("%Y%m%d-%H%M%S"))))
        });
        let config = CaptureConfig {
            region: self.settings.region.clone(),
            source,
            record_to,
            data: self.data.clone(),
        };
        self.log(format!(
            "starting capture — region {}, {:?}",
            config.region, config.source
        ));
        self.session = Some(CaptureSession::start(config));
        self.phase = Phase::Starting;
        self.show_capture_settings = false;
    }

    fn stop(&mut self) {
        if let Some(s) = &self.session {
            s.request_stop();
        }
    }

    fn pump(&mut self, ctx: &egui::Context) {
        // App self-update check / install.
        if let Some(rx) = &self.app_update_rx {
            if let Ok(status) = rx.try_recv() {
                self.app_update_rx = None;
                match &status {
                    AppUpdate::Available(v) => self.log(format!("Hollow Archive v{v} is available")),
                    AppUpdate::Installed(v) => {
                        self.log(format!("updated to v{v}; restart to use it"));
                        self.toast(ctx, format!("Updated to v{v} — restart Hollow Archive to finish."));
                    }
                    AppUpdate::Failed(e) => self.log(format!("warning: app update: {e}")),
                    _ => {}
                }
                self.app_update = status;
            }
        }

        // Data-file update check / install.
        if let Some(rx) = &self.update_rx {
            if let Ok(status) = rx.try_recv() {
                self.update_rx = None;
                match &status {
                    UpdateStatus::Available(v) => self.log(format!("data files for game {v} are available upstream")),
                    UpdateStatus::Installed(v) => {
                        self.log(format!("downloaded data files for {v}"));
                        self.reload_data_when_idle = true;
                    }
                    UpdateStatus::Failed(e) => self.log(format!("warning: data update: {e}")),
                    _ => {}
                }
                self.update = status;
            }
        }
        if self.reload_data_when_idle && self.session.is_none() {
            self.reload_data_when_idle = false;
            self.data = Arc::new(datafiles::load());
            self.regions = self.data.datamine.regions().map(str::to_string).collect();
            self.log(format!("now using data files for {}", self.data.version));
            self.toast(ctx, format!("Data files updated to {}", self.data.version));
        }

        let Some(session) = &self.session else { return };
        let mut events = Vec::new();
        while let Ok(ev) = session.events.try_recv() {
            events.push(ev);
        }
        let finished = session.is_finished();
        for ev in events {
            self.handle(ev);
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

    fn handle(&mut self, ev: CaptureEvent) {
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
                    self.phase = match self.phase {
                        Phase::Handshake => Phase::Error(
                            "Saw game traffic but could not decrypt it. Wrong region, or the game was already past the login screen — log out and back in.".into(),
                        ),
                        _ => Phase::Idle,
                    };
                }
            }
        }
    }

    fn export_json(&self) -> String {
        let zod = export(&self.acc.data, &self.data.gamedata, &self.settings.export);
        serde_json::to_string_pretty(&zod).unwrap_or_default()
    }

    // ---- hero (left) --------------------------------------------------------

    fn hero(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BG);

        #[cfg(hero_image)]
        {
            let img = egui::Image::new(egui::include_image!("../assets/hero.png"));
            if let Ok(egui::load::TexturePoll::Ready { texture }) = img.load_for_size(ui.ctx(), rect.size()) {
                // Cover-crop, biased toward the top-centre where the Hollow is.
                let scale = (rect.width() / texture.size.x).max(rect.height() / texture.size.y);
                let vw = rect.width() / (texture.size.x * scale);
                let vh = rect.height() / (texture.size.y * scale);
                let uv = Rect::from_min_size(Pos2::new((1.0 - vw) * 0.5, (1.0 - vh) * 0.35), Vec2::new(vw, vh));
                painter.image(texture.id, rect, uv, Color32::from_gray(165));
            }
        }
        #[cfg(not(hero_image))]
        {
            // No licensed art available: a quiet gradient and a dark disc standing in for the Hollow.
            gradient(
                &painter,
                rect,
                [
                    theme::BG,
                    theme::BG,
                    Color32::from_rgb(0x12, 0x30, 0x36),
                    Color32::from_rgb(0x0C, 0x22, 0x26),
                ],
            );
            let c = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.42);
            painter.circle_filled(c, rect.width() * 0.34, Color32::from_rgb(0x05, 0x0A, 0x0B));
            painter.circle_stroke(
                c,
                rect.width() * 0.34,
                egui::Stroke::new(2.0_f32, Color32::from_rgb(0x1E, 0x4E, 0x50)),
            );
        }

        // Fade into the panel on the right and into ink at the bottom for the HUD.
        let t = Color32::TRANSPARENT;
        let fade_w = rect.width() * 0.28;
        gradient(
            &painter,
            Rect::from_min_max(Pos2::new(rect.right() - fade_w, rect.top()), rect.max),
            [t, theme::PANEL, t, theme::PANEL],
        );
        let hud_h = rect.height() * 0.42;
        let ink = Color32::from_rgba_unmultiplied(theme::INK.r(), theme::INK.g(), theme::INK.b(), 235);
        gradient(
            &painter,
            Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - hud_h), rect.max),
            [t, t, ink, ink],
        );
        let top_ink = Color32::from_rgba_unmultiplied(theme::INK.r(), theme::INK.g(), theme::INK.b(), 150);
        gradient(
            &painter,
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 120.0)),
            [top_ink, top_ink, t, t],
        );

        // Title.
        let title_pos = rect.left_top() + Vec2::new(26.0, 22.0);
        let title_font = FontId::new(46.0, FontFamily::Name(theme::DISPLAY.into()));
        painter.text(
            title_pos + Vec2::new(2.0, 3.0),
            Align2::LEFT_TOP,
            "HOLLOW ARCHIVE",
            title_font.clone(),
            Color32::from_black_alpha(160),
        );
        painter.text(title_pos, Align2::LEFT_TOP, "HOLLOW ARCHIVE", title_font, theme::TEXT);
        painter.text(
            title_pos + Vec2::new(2.0, 58.0),
            Align2::LEFT_TOP,
            "INTER-KNOT FIELD TERMINAL",
            FontId::new(12.0, FontFamily::Name(theme::DISPLAY.into())),
            theme::TEAL,
        );

        // Status HUD.
        let phase = self.phase.clone();
        let base = Pos2::new(rect.left() + 26.0, rect.bottom() - 26.0);
        let hint_font = FontId::new(13.5, FontFamily::Proportional);
        let hint_galley = painter.layout(phase.hint(), hint_font, theme::TEXT, rect.width() - 52.0);
        let hint_top = base.y - hint_galley.size().y;
        painter.galley(Pos2::new(base.x, hint_top), hint_galley, theme::TEXT);
        let label_y = hint_top - 8.0;
        let lit = !phase.busy() || (ui.input(|i| i.time) * 2.0).fract() < 0.5;
        let dot = Pos2::new(base.x + 7.0, label_y - 15.0);
        if lit {
            painter.circle_filled(dot, 6.0, phase.color());
        } else {
            painter.circle_stroke(dot, 6.0, egui::Stroke::new(1.5_f32, phase.color()));
        }
        painter.text(
            Pos2::new(base.x + 22.0, label_y),
            Align2::LEFT_BOTTOM,
            phase.label(),
            FontId::new(30.0, FontFamily::Name(theme::DISPLAY.into())),
            phase.color(),
        );
        if phase.busy() {
            painter.text(
                Pos2::new(rect.right() - 20.0, label_y - 4.0),
                Align2::RIGHT_BOTTOM,
                format!("{} pkts", self.datagrams),
                FontId::new(12.0, FontFamily::Monospace),
                theme::MUTED,
            );
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }

        // The art doubles as the title bar.
        let r = ui.interact(rect, ui.id().with("hero-drag"), egui::Sense::drag());
        if r.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    // ---- panel (right) ------------------------------------------------------

    fn panel(&mut self, ui: &mut egui::Ui) {
        // Title-bar strip: drag + close.
        let strip = ui.available_rect_before_wrap();
        let strip = Rect::from_min_size(strip.min, Vec2::new(strip.width(), 34.0));
        let r = ui.interact(strip, ui.id().with("panel-drag"), egui::Sense::drag());
        if r.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if theme::icon_button(ui, ic::ICON_CLOSE, "Close").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.add_space(6.0);

        self.capture_section(ui);
        theme::thin_separator(ui);
        self.export_section(ui);
        theme::thin_separator(ui);
        self.log_section(ui);

        // Footer pinned to the bottom.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(2.0);
            self.footer(ui);
        });
    }

    fn heading(ui: &mut egui::Ui, text: &str, icons: impl FnOnce(&mut egui::Ui)) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(text).heading().color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), icons);
        });
    }

    fn capture_section(&mut self, ui: &mut egui::Ui) {
        let busy = self.phase.busy();
        let mut start = false;
        let mut stop = false;
        let mut toggle_settings = false;
        let complete = self.acc.complete();
        Self::heading(ui, "Packet Capture", |ui| {
            if theme::icon_button(ui, ic::ICON_SETTINGS, "Capture settings").clicked() {
                toggle_settings = true;
            }
            if busy {
                if theme::icon_button(ui, ic::ICON_STOP, "Stop capture").clicked() {
                    stop = true;
                }
            } else {
                let tip = if complete { "Capture again" } else { "Start capture" };
                if theme::icon_button_accent(ui, ic::ICON_PLAY_ARROW, tip).clicked() {
                    start = true;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.add_space(14.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, self.phase.color());
            ui.label(RichText::new(self.phase.short()).small().color(self.phase.color()));
        });

        let d = &self.acc.data;
        for (name, n) in [
            ("Agents", d.agents.len()),
            ("W-Engines", d.wengines.len()),
            ("Drive Discs", d.discs.len()),
        ] {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new("–").color(theme::MUTED));
                ui.label(RichText::new(name).color(theme::TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    let color = if n > 0 { theme::TEAL } else { theme::MUTED };
                    ui.label(
                        RichText::new(if n > 0 { n.to_string() } else { "—".into() })
                            .monospace()
                            .color(color),
                    );
                });
            });
        }

        if toggle_settings {
            self.show_capture_settings = !self.show_capture_settings;
        }
        if self.show_capture_settings {
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(theme::PANEL_RAISED)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .corner_radius(3)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Region").color(theme::MUTED));
                            egui::ComboBox::from_id_salt("region")
                                .selected_text(&self.settings.region)
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for r in &self.regions {
                                        ui.selectable_value(&mut self.settings.region, r.clone(), r);
                                    }
                                });
                        });
                        if cfg!(feature = "pcap") {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Backend").color(theme::MUTED));
                                ui.selectable_value(&mut self.settings.backend, Backend::Pktmon, "pktmon");
                                ui.selectable_value(&mut self.settings.backend, Backend::Pcap, "Npcap");
                            });
                        }
                        ui.checkbox(
                            &mut self.settings.save_captures,
                            "Save a copy of each capture (for bug reports)",
                        )
                        .on_hover_text(
                            Self::captures_dir()
                                .map(|d| d.display().to_string())
                                .unwrap_or_default(),
                        );
                    });
                });
        }

        if start {
            self.start();
        }
        if stop {
            self.stop();
        }
    }

    fn export_section(&mut self, ui: &mut egui::Ui) {
        let d = &self.acc.data;
        let have_data = !d.agents.is_empty() || !d.wengines.is_empty() || !d.discs.is_empty();
        let summary = if have_data {
            format!(
                "{} agents · {} W-Engines · {} discs ready",
                d.agents.len(),
                d.wengines.len(),
                d.discs.len()
            )
        } else {
            "Nothing captured yet".to_string()
        };
        let mut copy = false;
        let mut save = false;
        let mut toggle = false;
        Self::heading(ui, "Zenless Optimizer", |ui| {
            if theme::icon_button(ui, ic::ICON_TUNE, "Export filters").clicked() {
                toggle = true;
            }
            ui.add_enabled_ui(have_data, |ui| {
                if theme::icon_button(ui, ic::ICON_DOWNLOAD, "Save JSON file").clicked() {
                    save = true;
                }
                if theme::icon_button(ui, ic::ICON_CONTENT_COPY, "Copy JSON to clipboard").clicked() {
                    copy = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(RichText::new(summary).small().color(theme::MUTED));
        });

        if toggle {
            self.show_export_settings = !self.show_export_settings;
        }
        if self.show_export_settings {
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(theme::PANEL_RAISED)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .corner_radius(3)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let s = &mut self.settings.export;
                    egui::Grid::new("export-grid")
                        .num_columns(3)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("");
                            ui.label(RichText::new("min level").small().color(theme::MUTED));
                            ui.label(RichText::new("min rarity").small().color(theme::MUTED));
                            ui.end_row();
                            filter_row(
                                ui,
                                "Agents",
                                &mut s.export_agents,
                                &mut s.min_agent_level,
                                &mut s.min_agent_rarity,
                                60,
                                4,
                            );
                            filter_row(
                                ui,
                                "W-Engines",
                                &mut s.export_wengines,
                                &mut s.min_wengine_level,
                                &mut s.min_wengine_rarity,
                                60,
                                3,
                            );
                            filter_row(
                                ui,
                                "Drive Discs",
                                &mut s.export_discs,
                                &mut s.min_disc_level,
                                &mut s.min_disc_rarity,
                                15,
                                3,
                            );
                        });
                    ui.checkbox(
                        &mut s.pad_substats,
                        "Pad discs to 4 substats (zzz_packet_capture style)",
                    );
                });
        }

        if copy {
            let json = self.export_json();
            ui.ctx().copy_text(json);
            self.toast(ui.ctx(), "Copied — paste into Zenless Optimizer's import.");
        }
        if save {
            let name = format!("zzz_export_{}.json", chrono::Local::now().format("%Y-%m-%d_%H-%M"));
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(name)
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

    fn log_section(&mut self, ui: &mut egui::Ui) {
        let mut toggle = false;
        let open = self.settings.show_log;
        Self::heading(ui, "Capture Log", |ui| {
            let icon = if open {
                ic::ICON_EXPAND_LESS
            } else {
                ic::ICON_EXPAND_MORE
            };
            if theme::icon_button(ui, icon, "Show / hide log").clicked() {
                toggle = true;
            }
        });
        if toggle {
            self.settings.show_log = !self.settings.show_log;
        }
        if self.settings.show_log {
            let max_h = (ui.available_height() - 40.0).max(60.0);
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
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
        } else if let Some(last) = self.log.last() {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(last).monospace().small().color(theme::MUTED));
            });
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(theme::MUTED),
            );
            ui.label(RichText::new("·").small().color(theme::MUTED));
            ui.label(
                RichText::new(format!("game data {}", self.data.version))
                    .small()
                    .color(theme::MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                match self.app_update.clone() {
                    AppUpdate::Available(v) => {
                        let btn = egui::Button::new(
                            RichText::new(format!("{} v{v}", ic::ICON_UPGRADE))
                                .small()
                                .color(theme::INK),
                        )
                        .fill(theme::AMBER)
                        .stroke(egui::Stroke::NONE);
                        if ui
                            .add(btn)
                            .on_hover_text("Download and install the new version")
                            .clicked()
                        {
                            self.app_update = AppUpdate::Installing(v.clone());
                            self.app_update_rx = Some(update::install_in_background());
                        }
                    }
                    AppUpdate::Installing(v) => {
                        ui.label(RichText::new(format!("installing v{v}…")).small().color(theme::AMBER));
                        ui.ctx().request_repaint_after(Duration::from_millis(300));
                    }
                    AppUpdate::Installed(_) => {
                        ui.label(RichText::new("restart to finish update").small().color(theme::TEAL));
                    }
                    _ => {}
                }
                match self.update.clone() {
                    UpdateStatus::Available(v) => {
                        let btn = egui::Button::new(
                            RichText::new(format!("{} Update to {v}", ic::ICON_SYSTEM_UPDATE_ALT))
                                .small()
                                .color(theme::INK),
                        )
                        .fill(theme::AMBER)
                        .stroke(egui::Stroke::NONE);
                        if ui
                            .add(btn)
                            .on_hover_text("Download the latest datamine/schema/name tables")
                            .clicked()
                        {
                            self.update = UpdateStatus::Downloading(v.clone());
                            self.update_rx = Some(datafiles::install_in_background());
                        }
                    }
                    UpdateStatus::Downloading(v) => {
                        ui.label(RichText::new(format!("downloading {v}…")).small().color(theme::AMBER));
                        ui.ctx().request_repaint_after(Duration::from_millis(300));
                    }
                    UpdateStatus::Installed(_) if self.session.is_some() => {
                        ui.label(
                            RichText::new("update applies after capture")
                                .small()
                                .color(theme::MUTED),
                        );
                    }
                    UpdateStatus::Failed(_) => {
                        ui.label(RichText::new("data check failed").small().color(theme::MUTED))
                            .on_hover_text("See the capture log");
                    }
                    _ => {}
                }
            });
        });
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        if ctx.input(|i| i.time) - at > 3.5 {
            self.toast = None;
            return;
        }
        egui::Area::new("toast".into())
            .anchor(Align2::CENTER_BOTTOM, [0.0, -18.0])
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

fn filter_row(
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
            .width(48.0)
            .show_ui(ui, |ui| {
                for r in min_r..=5 {
                    ui.selectable_value(min_rarity, r, rarity_name(r));
                }
            });
    });
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

/// Four-corner gradient: [top-left, top-right, bottom-left, bottom-right].
fn gradient(painter: &egui::Painter, rect: Rect, c: [Color32; 4]) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), c[0]);
    mesh.colored_vertex(rect.right_top(), c[1]);
    mesh.colored_vertex(rect.left_bottom(), c[2]);
    mesh.colored_vertex(rect.right_bottom(), c[3]);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(Shape::mesh(mesh));
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::PANEL))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let hero_w = (full.width() * HERO_FRACTION).round();
                let hero_rect = Rect::from_min_size(full.min, Vec2::new(hero_w, full.height()));
                let panel_rect = Rect::from_min_max(Pos2::new(full.min.x + hero_w, full.min.y), full.max);

                self.hero(ui, hero_rect);
                let inner = panel_rect.shrink2(Vec2::new(18.0, 12.0));
                ui.scope_builder(
                    UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)),
                    |ui| {
                        self.panel(ui);
                    },
                );
            });
        self.toast_ui(ctx);
    }
}
