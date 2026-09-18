//! Palette from specs/ui.md: night-city teal on near-black, amber neon for actions.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x12, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x10, 0x1E, 0x20);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(0x16, 0x28, 0x2A);
pub const BORDER: Color32 = Color32::from_rgb(0x1E, 0x3A, 0x3C);
pub const TEXT: Color32 = Color32::from_rgb(0xC8, 0xE3, 0xE1);
pub const MUTED: Color32 = Color32::from_rgb(0x6C, 0x96, 0x94);
pub const TEAL: Color32 = Color32::from_rgb(0x35, 0xC4, 0xB4);
pub const AMBER: Color32 = Color32::from_rgb(0xF3, 0xB3, 0x3A);
pub const AMBER_DIM: Color32 = Color32::from_rgb(0x9C, 0x72, 0x22);
pub const RED: Color32 = Color32::from_rgb(0xE2, 0x5A, 0x44);
pub const INK: Color32 = Color32::from_rgb(0x08, 0x0E, 0x10);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = INK;
    v.faint_bg_color = PANEL;
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    v.widgets.inactive.bg_fill = PANEL_RAISED;
    v.widgets.inactive.weak_bg_fill = PANEL_RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x1C, 0x34, 0x36);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x1C, 0x34, 0x36);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TEAL);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.active.bg_fill = Color32::from_rgb(0x24, 0x44, 0x46);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x24, 0x44, 0x46);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, TEAL);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.selection.bg_fill = Color32::from_rgb(0x1E, 0x5C, 0x58);
    v.selection.stroke = Stroke::new(1.0_f32, TEAL);
    v.hyperlink_color = TEAL;
    v.warn_fg_color = AMBER;
    v.error_fg_color = RED;
    let r = CornerRadius::same(3);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.window_corner_radius = r;
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    ctx.set_style(style);
}

/// A bordered, filled section with a small uppercase title.
pub fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).small().color(MUTED).strong());
            ui.add_space(4.0);
            add(ui)
        })
        .inner
}

/// Amber primary action button.
pub fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let (fill, fg) = if enabled { (AMBER, INK) } else { (AMBER_DIM, PANEL) };
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).strong().color(fg))
            .fill(fill)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(120.0, 30.0)),
    )
}
