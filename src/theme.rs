//! Shared visual roles for the photo workspace and its settings.
use egui::{Color32, FontId, TextStyle, Vec2};
pub const BACKGROUND: Color32 = Color32::from_rgb(13, 19, 27);
pub const SURFACE: Color32 = Color32::from_rgb(22, 30, 40);
pub const TEXT: Color32 = Color32::from_rgb(231, 237, 244);
pub const MUTED: Color32 = Color32::from_rgb(160, 176, 192);
pub const BORDER: Color32 = Color32::from_rgb(42, 55, 70);
pub const ACCENT: Color32 = Color32::from_rgb(83, 198, 177);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(30, 91, 85);
pub const WARNING: Color32 = Color32::from_rgb(237, 190, 106);

pub fn apply(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(24.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(13.0, 7.0);
    style.spacing.interact_size = Vec2::new(32.0, 30.0);
    style.spacing.slider_width = 118.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.selection.bg_fill = ACCENT_DARK;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, MUTED);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 46, 60);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 46, 60);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 65, 80);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 65, 80);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.active.bg_fill = ACCENT_DARK;
    style.visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);
    ctx.set_style(style);
}

pub fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(5.0);
    ui.label(egui::RichText::new(label).size(16.0).strong());
}
