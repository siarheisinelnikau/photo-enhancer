//! Shared visual roles for the photo workspace and its settings.
use egui::{Color32, FontId, TextStyle, Vec2};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    #[default]
    #[serde(other)]
    Dark,
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_fill: Color32,
    pub warning: Color32,
    pub control: Color32,
    pub hover: Color32,
    pub checker: [u8; 2],
}
impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: Color32::from_rgb(13, 19, 27),
                surface: Color32::from_rgb(22, 30, 40),
                text: Color32::from_rgb(231, 237, 244),
                muted: Color32::from_rgb(160, 176, 192),
                border: Color32::from_rgb(42, 55, 70),
                accent: Color32::from_rgb(83, 198, 177),
                accent_fill: Color32::from_rgb(30, 91, 85),
                warning: Color32::from_rgb(237, 190, 106),
                control: Color32::from_rgb(34, 46, 60),
                hover: Color32::from_rgb(46, 65, 80),
                checker: [40, 49],
            },
            Self::Light => Palette {
                background: Color32::from_rgb(239, 244, 247),
                surface: Color32::from_rgb(252, 253, 254),
                text: Color32::from_rgb(25, 39, 50),
                muted: Color32::from_rgb(79, 98, 112),
                border: Color32::from_rgb(188, 205, 214),
                accent: Color32::from_rgb(0, 108, 94),
                accent_fill: Color32::from_rgb(183, 226, 216),
                warning: Color32::from_rgb(134, 77, 0),
                control: Color32::from_rgb(226, 235, 240),
                hover: Color32::from_rgb(208, 226, 232),
                checker: [216, 233],
            },
        }
    }
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let colors = theme.palette();
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
    style.visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    style.visuals.override_text_color = Some(colors.text);
    style.visuals.panel_fill = colors.background;
    style.visuals.window_fill = colors.surface;
    style.visuals.extreme_bg_color = colors.background;
    style.visuals.faint_bg_color = colors.surface;
    style.visuals.window_stroke = egui::Stroke::new(1.0, colors.border);
    style.visuals.selection.bg_fill = colors.accent_fill;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, colors.text);
    style.visuals.hyperlink_color = colors.accent;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors.border);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors.muted);
    style.visuals.widgets.inactive.bg_fill = colors.control;
    style.visuals.widgets.inactive.weak_bg_fill = colors.control;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors.text);
    style.visuals.widgets.hovered.bg_fill = colors.hover;
    style.visuals.widgets.hovered.weak_bg_fill = colors.hover;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, colors.text);
    style.visuals.widgets.active.bg_fill = colors.accent_fill;
    style.visuals.widgets.active.weak_bg_fill = colors.accent_fill;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, colors.text);
    ctx.set_style(style);
}

pub fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(5.0);
    ui.label(egui::RichText::new(label).size(16.0).strong());
}
