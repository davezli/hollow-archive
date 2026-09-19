//! Palette from specs/ui.md: night-city teal on near-black, amber neon for actions.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle, Visuals,
};

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x12, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x0E, 0x1A, 0x1C);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(0x16, 0x28, 0x2A);
pub const BORDER: Color32 = Color32::from_rgb(0x1E, 0x3A, 0x3C);
pub const TEXT: Color32 = Color32::from_rgb(0xD6, 0xEA, 0xE8);
pub const MUTED: Color32 = Color32::from_rgb(0x6C, 0x96, 0x94);
pub const TEAL: Color32 = Color32::from_rgb(0x35, 0xC4, 0xB4);
pub const AMBER: Color32 = Color32::from_rgb(0xF3, 0xB3, 0x3A);
pub const RED: Color32 = Color32::from_rgb(0xE2, 0x5A, 0x44);
pub const INK: Color32 = Color32::from_rgb(0x08, 0x0E, 0x10);

pub const DISPLAY: &str = "display";

pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        DISPLAY.into(),
        std::sync::Arc::new(FontData::from_static(include_bytes!("../assets/Oswald.ttf"))),
    );
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY.into()), vec![DISPLAY.into()]);
    ctx.set_fonts(fonts);
    egui_material_icons::initialize(ctx);

    let mut v = Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL_RAISED;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.extreme_bg_color = INK;
    v.faint_bg_color = PANEL_RAISED;
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
    v.popup_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 22.0;
    ctx.set_style(style);
}

/// Frameless icon button (Material Icons glyph).
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    let btn = egui::Button::new(RichText::new(icon).size(18.0).color(MUTED))
        .frame(false)
        .min_size(egui::vec2(26.0, 26.0));
    let r = ui.add(btn).on_hover_text(tip);
    if r.hovered() {
        ui.painter().text(
            r.rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::new(18.0, FontFamily::Proportional),
            TEXT,
        );
    }
    r
}

/// Same, in the accent colour, for the one primary action.
pub fn icon_button_accent(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    let fill = if r.hovered() {
        Color32::from_rgb(0xFF, 0xC4, 0x55)
    } else {
        AMBER
    };
    ui.painter().circle_filled(rect.center(), 15.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::new(20.0, FontFamily::Proportional),
        INK,
    );
    r.on_hover_text(tip)
}

pub fn thin_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0_f32, BORDER));
    ui.add_space(4.0);
}
