#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod capture;
mod host;
mod i18n;
mod preferences;
mod theme;

use egui::{Color32, RichText, Vec2};
use i18n::{Language, Message, tr};
use photo_enhancer::{
    engine::{self, Adjustments, OutputFormat},
    inference::{Capability, LocalEnhancementBackend, UnavailableBackend},
    updates,
    worker::{self, Worker},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver},
    },
};

fn main() -> anyhow::Result<()> {
    if updates::helper_cli().map_err(anyhow::Error::msg)? {
        return Ok(());
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--cleanup-update")
    {
        return host::run(None);
    }
    match capture::parse_args()? {
        capture::Launch::Normal => host::run(None),
        capture::Launch::Capture(options) => capture::save(options),
        capture::Launch::Diagnose { output, input } => host::run(Some((output, input))),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    startup_checks: bool,
    format: OutputFormat,
    quality: u8,
    language: Language,
    theme: theme::Theme,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            startup_checks: true,
            format: OutputFormat::Png,
            quality: 94,
            language: Language::English,
            theme: theme::Theme::Dark,
        }
    }
}

#[derive(PartialEq)]
enum Compare {
    Split,
    Before,
    After,
}
enum Dialog {
    Open,
    AddBatch,
    Folder,
    Export(String),
}
enum Selection {
    Open(PathBuf),
    Batch(Vec<PathBuf>),
    Folder(PathBuf),
    Export(PathBuf),
    Cancelled,
}
struct QueueItem {
    path: PathBuf,
    state: Message,
}

struct PhotoEnhancer {
    worker: Worker,
    updater: updates::Updater,
    preferences: Preferences,
    preferences_error: Option<String>,
    settings: Adjustments,
    preview_settings: Option<Adjustments>,
    pending_preview: Option<Adjustments>,
    current: Option<PathBuf>,
    size: Option<(u32, u32)>,
    before: Option<egui::TextureHandle>,
    after: Option<egui::TextureHandle>,
    compare: Compare,
    split: f32,
    zoom: f32,
    pan: Vec2,
    busy: bool,
    message: Message,
    message_error: bool,
    settings_open: bool,
    dialog: Option<Receiver<Selection>>,
    queue: Vec<QueueItem>,
    output_folder: Option<PathBuf>,
    batch_running: bool,
    batch_index: usize,
    batch_open: bool,
    update_busy: bool,
    installing: bool,
    update_status: Message,
    update_failed: bool,
    available: Option<updates::Available>,
    installed: bool,
    prepared_update: Option<updates::Prepared>,
    update_progress: f32,
}
impl PhotoEnhancer {
    fn new(ctx: egui::Context, preferences: Preferences) -> Self {
        theme::apply(&ctx, preferences.theme);
        let image_ctx = ctx.clone();
        let update_ctx = ctx.clone();
        let worker = Worker::new(move || image_ctx.request_repaint());
        let updater = updates::start(move || update_ctx.request_repaint());
        let startup = preferences.startup_checks;
        let mut app = Self {
            worker,
            updater,
            preferences,
            preferences_error: None,
            settings: Adjustments::default(),
            preview_settings: None,
            pending_preview: None,
            current: None,
            size: None,
            before: None,
            after: None,
            compare: Compare::Split,
            split: 0.5,
            zoom: 1.0,
            pan: Vec2::ZERO,
            busy: false,
            message: Message::key(
                "Open a PNG or JPEG to begin. Processing stays on this computer.",
            ),
            message_error: false,
            settings_open: false,
            dialog: None,
            queue: Vec::new(),
            output_folder: None,
            batch_running: false,
            batch_index: 0,
            batch_open: false,
            update_busy: false,
            installing: false,
            update_status: Message::key("Not checked"),
            update_failed: false,
            available: None,
            installed: false,
            prepared_update: None,
            update_progress: 0.0,
        };
        if startup {
            app.check_updates();
        }
        app
    }
    fn command(&mut self, command: worker::Command) {
        match self.worker.submit(command) {
            Ok(()) => {
                self.busy = true;
                self.message_error = false;
            }
            Err(e) => {
                self.message = e.to_string().into();
                self.message_error = true;
            }
        }
    }
    fn dialog(&mut self, kind: Dialog, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let lang = self.preferences.language;
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("file-dialog".into())
            .spawn(move || {
                let selected = match kind {
                    Dialog::Open => rfd::FileDialog::new()
                        .add_filter(tr(lang, "Photos"), &["png", "jpg", "jpeg"])
                        .pick_file()
                        .map(Selection::Open),
                    Dialog::AddBatch => rfd::FileDialog::new()
                        .add_filter(tr(lang, "Photos"), &["png", "jpg", "jpeg"])
                        .pick_files()
                        .map(Selection::Batch),
                    Dialog::Folder => rfd::FileDialog::new().pick_folder().map(Selection::Folder),
                    Dialog::Export(name) => rfd::FileDialog::new()
                        .set_file_name(name)
                        .save_file()
                        .map(Selection::Export),
                }
                .unwrap_or(Selection::Cancelled);
                let _ = tx.send(selected);
                ctx.request_repaint();
            })
            .expect("file dialog thread");
        self.dialog = Some(rx);
    }
    fn texture(ctx: &egui::Context, name: &str, image: &image::RgbaImage) -> egui::TextureHandle {
        ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        )
    }
    fn receive(&mut self, ctx: &egui::Context) {
        if let Some(selection) = self.dialog.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.dialog = None;
            match selection {
                Selection::Open(path) => {
                    self.message = Message::key("Decoding image…");
                    self.command(worker::Command::Open(path));
                }
                Selection::Batch(paths) => {
                    self.batch_open = true;
                    for path in paths {
                        if self.queue.len() == 64 {
                            self.message = Message::key(
                                "Queue limited to 64 images. Add the rest after this batch.",
                            );
                            break;
                        }
                        if !self.queue.iter().any(|q| q.path == path) {
                            self.queue.push(QueueItem {
                                path,
                                state: Message::key("Waiting"),
                            });
                        }
                    }
                }
                Selection::Folder(path) => self.output_folder = Some(path),
                Selection::Export(mut path) => {
                    path.set_extension(self.preferences.format.extension());
                    if let Some(input) = self.current.clone() {
                        self.message = Message::key("Exporting full-resolution image…");
                        self.command(worker::Command::Export {
                            input,
                            output: path,
                            settings: self.settings.clone(),
                            format: self.preferences.format,
                            quality: self.preferences.quality,
                        });
                    }
                }
                Selection::Cancelled => {}
            }
        }
        if let Ok(event) = self.worker.rx.try_recv() {
            self.busy = false;
            let mut continue_batch = false;
            match event {
                worker::Event::Opened { path, size, before } => {
                    self.after = None;
                    self.preview_settings = None;
                    self.pending_preview = None;
                    self.current = Some(path);
                    self.size = Some(size);
                    self.before = Some(Self::texture(ctx, "before", &before));
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                    self.message =
                        Message::key("Image ready. Adjust controls, then click Render preview.");
                }
                worker::Event::Previewed(image) => {
                    self.after = Some(Self::texture(ctx, "after", &image));
                    self.preview_settings = self.pending_preview.take();
                    self.message = Message::key(
                        "Preview ready. Export processes the original at full resolution.",
                    );
                }
                worker::Event::Exported(path) => {
                    self.message = Message::with("Saved {path}", "path", path.display());
                    if self.batch_running && !self.worker.cancel.load(Ordering::Relaxed) {
                        self.queue[self.batch_index].state = Message::key("Saved");
                        continue_batch = true;
                    } else if self.batch_running {
                        self.queue[self.batch_index].state = Message::key("Saved; batch stopped");
                        self.batch_running = false;
                    }
                }
                worker::Event::Failed(error) => {
                    self.message_error = true;
                    self.pending_preview = None;
                    self.message = error.clone().into();
                    if self.batch_running {
                        self.queue[self.batch_index].state = error.into();
                        if self.worker.cancel.load(Ordering::Relaxed) {
                            self.batch_running = false;
                        } else {
                            continue_batch = true;
                        }
                    }
                }
            }
            if continue_batch {
                self.batch_index += 1;
                self.next_batch();
            }
        }
        while let Ok(event) = self.updater.events.try_recv() {
            match event {
                updates::Event::Checked(result) => {
                    self.update_busy = false;
                    match result {
                        Ok(Some(update)) => {
                            self.update_status = Message::with(
                                "Version {version} is available",
                                "version",
                                &update.version,
                            );
                            self.available = Some(update);
                        }
                        Ok(None) => {
                            self.update_status = Message::key("You are up to date");
                            self.available = None;
                        }
                        Err(error) => {
                            self.update_status = error.into();
                            self.update_failed = true;
                        }
                    }
                }
                updates::Event::Progress { received, total } => {
                    self.update_progress = received as f32 / total.max(1) as f32;
                }
                updates::Event::Ready(result) => {
                    self.update_busy = false;
                    self.installing = false;
                    match result {
                        Ok(prepared) => {
                            self.prepared_update = Some(prepared);
                            self.installed = true;
                            self.update_status = Message::key(
                                "Download verified. Install and restart when your work is saved.",
                            );
                        }
                        Err(error) => {
                            self.update_status = error.into();
                            self.update_failed = true;
                        }
                    }
                }
            }
        }
    }
    fn next_batch(&mut self) {
        if self.batch_index >= self.queue.len() {
            self.batch_running = false;
            self.message =
                Message::key("Batch finished. Review each queue item for saved files or errors.");
            return;
        }
        let input = self.queue[self.batch_index].path.clone();
        let output = engine::output_path(
            &input,
            self.output_folder.as_ref().unwrap(),
            self.preferences.format,
        );
        match output {
            Ok(output) => {
                self.queue[self.batch_index].state = Message::key("Processing");
                self.command(worker::Command::Export {
                    input,
                    output,
                    settings: self.settings.clone(),
                    format: self.preferences.format,
                    quality: self.preferences.quality,
                });
                if !self.busy {
                    self.batch_running = false;
                }
            }
            Err(e) => {
                self.queue[self.batch_index].state = e.to_string().into();
                self.batch_running = false;
            }
        }
    }
    fn check_updates(&mut self) {
        match self.updater.check() {
            Ok(()) => {
                self.update_failed = false;
                self.update_busy = true;
                self.update_status = Message::key("Checking public GitHub releases…");
            }
            Err(error) => {
                self.update_failed = true;
                self.update_status = error.into();
            }
        }
    }
    fn download_update(&mut self, update: updates::Available) {
        match self.updater.download(update) {
            Ok(()) => {
                self.update_busy = true;
                self.installing = true;
                self.update_failed = false;
                self.update_progress = 0.0;
                self.update_status = Message::key("Downloading and verifying update…");
            }
            Err(error) => {
                self.update_failed = true;
                self.update_status = error.into();
            }
        }
    }
    fn adjustment(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) {
        ui.label(label);
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(value, range.clone()).show_value(false));
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(0.01)
                    .fixed_decimals(2),
            );
        });
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        let lang = self.preferences.language;
        let colors = self.preferences.theme.palette();
        let enabled = !self.busy && !self.installing && self.dialog.is_none();
        let form_height = (ui.available_height() - 130.0).max(150.0);
        egui::ScrollArea::vertical().id_salt("adjustment_form").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).max_height(form_height).auto_shrink([false,false]).show(ui,|ui| {
            ui.heading(tr(lang, "Adjust"));
            ui.add_enabled_ui(enabled,|ui| {
                theme::section(ui,tr(lang, "Light & color"));
                Self::adjustment(ui,tr(lang, "Exposure"),&mut self.settings.exposure,-3.0..=3.0);
                Self::adjustment(ui,tr(lang, "Saturation"),&mut self.settings.saturation,0.0..=2.0);
                Self::adjustment(ui,tr(lang, "Warmth"),&mut self.settings.warmth,-1.0..=1.0);
                ui.add_space(5.0);ui.separator();
                theme::section(ui,tr(lang, "Detail"));
                Self::adjustment(ui,tr(lang, "Denoise"),&mut self.settings.denoise,0.0..=1.0);
                Self::adjustment(ui,tr(lang, "Sharpen"),&mut self.settings.sharpen,0.0..=2.0);
                ui.add_space(5.0);ui.separator();
                theme::section(ui,tr(lang, "Output"));
                egui::ComboBox::from_label(tr(lang, "Size")).selected_text(i18n::format(lang,"{scale}x enlargement",&[("scale",self.settings.scale.to_string())])).show_ui(ui,|ui| {
                    for scale in 1..=4 {ui.selectable_value(&mut self.settings.scale,scale,format!("{scale}x"));}
                }).response.on_hover_text(tr(lang, "Uses linear-light interpolation. Enlarging does not recover true lost detail."));
                if let Some((w,h))=self.size {match engine::dimensions(w,h,self.settings.scale) {
                    Ok((ow,oh))=>{ui.label(RichText::new(i18n::format(lang,"{width} x {height} pixels",&[("width",ow.to_string()),("height",oh.to_string())])).color(colors.muted));}
                    Err(error)=>{ui.colored_label(colors.warning,Message::from(error.to_string()).render(lang));}
                }}
                egui::ComboBox::from_label(tr(lang, "Format")).selected_text(match self.preferences.format {OutputFormat::Png=>tr(lang, "PNG · lossless"),OutputFormat::Jpeg=>"JPEG"}).show_ui(ui,|ui| {
                    ui.selectable_value(&mut self.preferences.format,OutputFormat::Png,tr(lang, "PNG · lossless / transparency"));
                    ui.selectable_value(&mut self.preferences.format,OutputFormat::Jpeg,tr(lang, "JPEG · opaque images"));
                });
                if self.preferences.format==OutputFormat::Jpeg {ui.add(egui::Slider::new(&mut self.preferences.quality,1..=100).text(tr(lang, "Quality")));}
                ui.add_space(8.0);
                let valid_size=self.size.is_some_and(|(w,h)|engine::dimensions(w,h,self.settings.scale).is_ok());
                if ui.add_enabled(valid_size,egui::Button::new(tr(lang, "Update preview")).min_size(egui::vec2(ui.available_width(),34.0))).clicked() {
                    let settings=self.settings.clone();self.pending_preview=Some(settings.clone());
                    self.command(worker::Command::Preview {input:self.current.clone().unwrap(),settings});self.message=Message::key("Updating preview…");
                }
                if self.after.is_some() && self.preview_settings.as_ref()!=Some(&self.settings) {ui.colored_label(colors.warning,tr(lang, "Adjustments changed. Update the preview."));}
                if ui.button(tr(lang, "Reset adjustments")).clicked() {self.settings=Adjustments::default();}
            });
        });
        ui.separator();
        if self.busy {
            ui.add(
                egui::ProgressBar::new(
                    self.worker.progress.load(Ordering::Relaxed) as f32 / 1000.0,
                )
                .show_percentage(),
            );
            if ui.button(tr(lang, "Cancel processing")).clicked() {
                self.worker.cancel.store(true, Ordering::Relaxed);
                self.message =
                    Message::key("Cancellation requested; waiting for the current codec or row…");
            }
        } else {
            let valid_size = self
                .size
                .is_some_and(|(w, h)| engine::dimensions(w, h, self.settings.scale).is_ok());
            if let Some((w, h)) = self.size
                && let Ok((ow, oh)) = engine::dimensions(w, h, self.settings.scale)
            {
                ui.label(
                    RichText::new(format!(
                        "{} · {ow} x {oh}",
                        self.preferences.format.extension().to_uppercase()
                    ))
                    .small()
                    .color(colors.muted),
                );
            }
            if ui
                .add_enabled(
                    enabled && valid_size,
                    egui::Button::new(RichText::new(tr(lang, "Export image…")).strong())
                        .fill(colors.accent_fill)
                        .min_size(egui::vec2(ui.available_width(), 38.0)),
                )
                .clicked()
            {
                let filename = engine::output_path(
                    self.current.as_ref().unwrap(),
                    std::path::Path::new(""),
                    self.preferences.format,
                )
                .unwrap()
                .to_string_lossy()
                .into_owned();
                self.dialog(Dialog::Export(filename), ui.ctx());
            }
            ui.label(
                RichText::new(tr(
                    lang,
                    "Creates a new file. Your original stays untouched.",
                ))
                .small()
                .color(colors.muted),
            );
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        let lang = self.preferences.language;
        let colors = self.preferences.theme.palette();
        ui.horizontal_wrapped(|ui| {
            let has_preview = self.after.is_some();
            ui.add_enabled_ui(has_preview, |ui| {
                ui.selectable_value(&mut self.compare, Compare::Split, tr(lang, "Compare"));
            });
            ui.selectable_value(&mut self.compare, Compare::Before, tr(lang, "Before"));
            ui.add_enabled_ui(has_preview, |ui| {
                ui.selectable_value(&mut self.compare, Compare::After, tr(lang, "After"));
            });
            ui.separator();
            if ui.button(tr(lang, "Fit")).clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            ui.add(
                egui::Slider::new(&mut self.zoom, 0.25..=4.0)
                    .text(tr(lang, "Zoom"))
                    .fixed_decimals(2),
            );
        });
        ui.add_space(5.0);
        ui.label(
            RichText::new(tr(
                lang,
                "Drag to pan · scroll to zoom · drag the divider to compare",
            ))
            .small()
            .color(colors.muted),
        );
        ui.add_space(5.0);
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            egui::Sense::drag(),
        );
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new("photo_canvas_rect"), rect));
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 6.0, colors.background);
        let Some(before) = &self.before else {
            for (key, size, offset, color) in [
                ("Start with a photo", 28.0, -40.0, colors.text),
                (
                    "Open a PNG or JPEG. Everything is processed locally.",
                    14.0,
                    10.0,
                    colors.muted,
                ),
            ] {
                let text = painter.layout(
                    tr(lang, key).to_owned(),
                    egui::FontId::proportional(size),
                    color,
                    (rect.width() - 36.0).max(1.0),
                );
                let position = egui::pos2(
                    rect.center().x - text.size().x / 2.0,
                    rect.center().y + offset,
                );
                painter.galley(position, text, color);
            }
            return;
        };
        let divider_x = rect.left() + rect.width() * self.split;
        let divider_rect = egui::Rect::from_center_size(
            egui::pos2(divider_x, rect.center().y),
            egui::vec2(18.0, rect.height()),
        );
        let divider = ui.interact(
            divider_rect,
            ui.id().with("comparison_divider"),
            if self.after.is_some() && self.compare == Compare::Split {
                egui::Sense::drag()
            } else {
                egui::Sense::hover()
            },
        );
        if divider.dragged() {
            if let Some(pointer) = divider.interact_pointer_pos() {
                self.split = ((pointer.x - rect.left()) / rect.width()).clamp(0.03, 0.97);
            }
        } else if response.dragged() {
            self.pan += response.drag_delta();
        }
        if self.after.is_some() && self.compare == Compare::Split {
            divider.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom;
                self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.25, 4.0);
                let cursor = response.hover_pos().unwrap_or(rect.center()) - rect.center();
                self.pan = cursor - (cursor - self.pan) * (self.zoom / old);
            }
        }
        let fit =
            (rect.width() / before.size_vec2().x).min(rect.height() / before.size_vec2().y) * 0.94;
        let image_rect = egui::Rect::from_center_size(
            rect.center() + self.pan,
            before.size_vec2() * fit * self.zoom,
        );
        let checker = image_rect.intersect(rect);
        for y in 0..(checker.height() / 18.0).ceil() as u32 {
            for x in 0..(checker.width() / 18.0).ceil() as u32 {
                let square = egui::Rect::from_min_size(
                    checker.min + Vec2::new(x as f32 * 18.0, y as f32 * 18.0),
                    Vec2::splat(18.0),
                )
                .intersect(checker);
                painter.rect_filled(
                    square,
                    0.0,
                    Color32::from_gray(colors.checker[((x + y) % 2) as usize]),
                );
            }
        }
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        if self.compare != Compare::After || self.after.is_none() {
            painter.image(before.id(), image_rect, uv, Color32::WHITE);
        }
        if self.compare != Compare::Before
            && let Some(after) = &self.after
        {
            let clip = if self.compare == Compare::Split {
                egui::Rect::from_min_max(
                    egui::pos2(rect.left() + rect.width() * self.split, rect.top()),
                    rect.max,
                )
            } else {
                rect
            };
            painter.with_clip_rect(clip.intersect(rect)).image(
                after.id(),
                image_rect,
                uv,
                Color32::WHITE,
            );
            if self.compare == Compare::Split {
                let x = rect.left() + rect.width() * self.split;
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    (1.5, colors.accent),
                );
            }
        }
        let tag = |position: egui::Pos2, text: &str, align: egui::Align2| {
            let galley = painter.layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(13.0),
                colors.text,
            );
            let anchor = align.anchor_size(position, galley.size() + egui::vec2(20.0, 12.0));
            painter.rect_filled(anchor, 4.0, colors.surface);
            painter.galley(anchor.min + egui::vec2(10.0, 6.0), galley, colors.text);
        };
        if self.compare != Compare::After || self.after.is_none() {
            tag(
                rect.left_top() + Vec2::splat(14.0),
                tr(lang, "BEFORE"),
                egui::Align2::LEFT_TOP,
            );
        }
        if self.after.is_some() && self.compare != Compare::Before {
            tag(
                rect.right_top() + Vec2::new(-14.0, 14.0),
                tr(lang, "AFTER"),
                egui::Align2::RIGHT_TOP,
            );
        }
    }
    fn batch_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.preferences.language;
        let colors = self.preferences.theme.palette();
        let enabled = !self.busy && self.dialog.is_none() && !self.installing;
        ui.horizontal(|ui| {
            ui.toggle_value(
                &mut self.batch_open,
                i18n::format(
                    lang,
                    "Batch export · {count} photos",
                    &[("count", self.queue.len().to_string())],
                ),
            );
            if ui
                .add_enabled(enabled, egui::Button::new(tr(lang, "Add photos…")))
                .clicked()
            {
                self.dialog(Dialog::AddBatch, ui.ctx());
            }
            if !self.queue.is_empty()
                && ui
                    .add_enabled(enabled, egui::Button::new(tr(lang, "Clear queue")))
                    .clicked()
            {
                self.queue.clear();
            }
        });
        if !self.batch_open {
            return;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(tr(lang, "Choose output folder…")),
                )
                .clicked()
            {
                self.dialog(Dialog::Folder, ui.ctx());
            }
            if ui
                .add_enabled(
                    enabled && !self.queue.is_empty() && self.output_folder.is_some(),
                    egui::Button::new(tr(lang, "Export batch")).fill(colors.accent_fill),
                )
                .clicked()
            {
                for item in &mut self.queue {
                    item.state = Message::key("Waiting");
                }
                self.batch_running = true;
                self.batch_index = 0;
                self.next_batch();
            }
            let destination = self
                .output_folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| tr(lang, "Choose where to save the enhanced copies.").into());
            ui.add(egui::Label::new(RichText::new(&destination).color(colors.muted)).truncate())
                .on_hover_text(destination);
        });
        egui::ScrollArea::vertical()
            .max_height(110.0)
            .show(ui, |ui| {
                for item in &self.queue {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [ui.available_width() * 0.45, 26.0],
                            egui::Label::new(
                                item.path.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            .truncate(),
                        )
                        .on_hover_text(item.path.display().to_string());
                        ui.add(egui::Label::new(item.state.render(lang)).truncate())
                            .on_hover_text(item.state.render(lang));
                    });
                }
            });
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        let lang = self.preferences.language;
        let colors = self.preferences.theme.palette();
        let mut open = self.settings_open;
        egui::Window::new(tr(lang, "Settings & updates")).id(egui::Id::new("Settings & updates")).open(&mut open).default_width(510.0).default_height(580.0).min_width(350.0)
            .default_pos(ctx.content_rect().center()-egui::vec2(265.0,300.0))
            .max_width((ctx.content_rect().width()-48.0).max(350.0))
            .max_height((ctx.content_rect().height()-64.0).max(240.0)).vscroll(true).show(ctx,|ui| {
                ui.heading("PhotoEnhancer");
                ui.label(RichText::new(i18n::format(lang,"Version {version} · Windows x64",&[("version",env!("CARGO_PKG_VERSION").into())])).color(colors.muted));
                ui.add_space(6.0);ui.separator();theme::section(ui,tr(lang, "Appearance"));
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr(lang,"Language"));
                    egui::ComboBox::from_id_salt("ui_language").selected_text(self.preferences.language.native_name()).show_ui(ui,|ui| {
                        for language in Language::ALL { ui.selectable_value(&mut self.preferences.language,language,language.native_name()); }
                    });
                });
                let previous_theme=self.preferences.theme;
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr(lang,"Theme"));
                    ui.selectable_value(&mut self.preferences.theme,theme::Theme::Light,tr(lang,"Light"));
                    ui.selectable_value(&mut self.preferences.theme,theme::Theme::Dark,tr(lang,"Dark"));
                });
                if previous_theme!=self.preferences.theme { theme::apply(ctx,self.preferences.theme); }
                ui.add_space(6.0);ui.separator();theme::section(ui,tr(lang, "Updates"));
                ui.checkbox(&mut self.preferences.startup_checks,tr(lang, "Check for updates on startup"));
                ui.label(RichText::new(tr(lang, "Only GitHub is contacted. Your photos stay on this computer.")).small().color(colors.muted));
                if let Some(error)=&self.preferences_error {ui.colored_label(colors.warning,format!("{}: {error}",tr(lang,"Settings could not be saved")));}
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.update_busy && !self.installed,egui::Button::new(tr(lang, "Check for updates"))).clicked() {self.check_updates();}
                    if !self.update_failed {ui.add(egui::Label::new(RichText::new(self.update_status.render(lang)).color(colors.muted)).truncate()).on_hover_text(self.update_status.render(lang));}
                });
                if self.update_failed {
                    ui.colored_label(colors.warning,if self.update_status.contains("404") {tr(lang, "No published release is available yet.")} else {tr(lang, "Updates are unavailable. Please try again later.")});
                    ui.collapsing(tr(lang, "Error details"),|ui| {ui.add(egui::Label::new(self.update_status.render(lang)).wrap());});
                }
                if let Some(update)=self.available.clone() {
                    ui.label(i18n::format(lang,"Version {version} is available",&[("version",update.version.to_string())]));
                    if ui.add_enabled(!self.update_busy && !self.busy && self.dialog.is_none() && !self.installed,egui::Button::new(tr(lang, "Download and install")).fill(colors.accent_fill)).clicked() {
                        self.download_update(update);
                    }
                }
                if self.installing {
                    ui.add(egui::ProgressBar::new(self.update_progress).show_percentage());
                    if ui.button(tr(lang, "Cancel download")).clicked() {self.updater.cancel_download();}
                }
                if self.installed && ui.add_enabled(!self.busy && self.dialog.is_none(), egui::Button::new(tr(lang, "Install and restart"))).clicked()
                    && let Some(prepared) = &mut self.prepared_update {
                    match updates::handoff(prepared) {
                        Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(error) => {self.update_failed=true;self.update_status=error.into();}
                    }
                }
                ui.add_space(6.0);ui.separator();theme::section(ui,tr(lang, "About photo enhancement"));
                ui.label(tr(lang, "Local filters for light, color, noise and sharpness. Enlargement uses standard interpolation."));
                if let Capability::Unavailable {..}=UnavailableBackend.capability() {ui.label(RichText::new(tr(lang, "AI enhancement is not included in this version.")).color(colors.muted));}
                ui.collapsing(tr(lang, "Supported images"),|ui| {
                    ui.label(tr(lang, "8-bit RGB or grayscale PNG and JPEG, up to 24 megapixels. Output is limited to 32 megapixels and the memory budget."));
                    ui.label(tr(lang, "Embedded RGB and grayscale color profiles are converted to sRGB automatically. CMYK, HDR and high-bit-depth inputs need conversion first. PNG preserves transparency; export removes original metadata."));
                });
                ui.hyperlink_to(tr(lang, "Project on GitHub"),updates::REPOSITORY);
                ui.add_space(6.0);ui.separator();
                ui.collapsing(tr(lang, "Rendering diagnostics"),|ui| {
                    ui.label(tr(lang, "CPU rasterization / Windows GDI"));
                    if let Some(metrics)=ctx.data(|data|data.get_temp::<host::FrameMetrics>(egui::Id::new("frame_metrics"))) && metrics.frames>0 {
                        ui.label(i18n::format(lang,"Last frame {last} ms · average {average} ms",&[("last",format!("{:.1}",metrics.last_ms)),("average",format!("{:.1}",metrics.total_ms/metrics.frames as f64))]));
                        ui.small(i18n::format(lang,"Includes UI, rasterization and presentation. Maximum {maximum} ms includes startup.",&[("maximum",format!("{:.1}",metrics.max_ms))]));
                        ui.small(i18n::format(lang,"PID {pid} · CPU time {seconds} s · {frames} rendered frames",&[("pid",std::process::id().to_string()),("seconds",format!("{:.3}",metrics.process_cpu_seconds)),("frames",metrics.frames.to_string())]));
                        ui.small(if metrics.graphics_modules.is_empty() {tr(lang, "OpenGL / Direct3D modules: none loaded").into()} else {i18n::format(lang,"Detected graphics modules: {modules}",&[("modules",metrics.graphics_modules.join(", "))])});
                    }
                });
                ui.collapsing(tr(lang, "Licenses & notices"),|ui| {
                    ui.label("PhotoEnhancer © 2026 contributors · MIT License");
                    static LINES:std::sync::OnceLock<Vec<&'static str>>=std::sync::OnceLock::new();
                    let lines=LINES.get_or_init(||include_str!("../docs/THIRD_PARTY_NOTICES.txt").lines().collect());
                    egui::ScrollArea::both().max_height(180.0).show_rows(ui,16.0,lines.len(),|ui,rows| {
                        for row in rows {ui.add(egui::Label::new(RichText::new(lines[row]).monospace().size(12.0)).wrap_mode(egui::TextWrapMode::Extend));}
                    });
                });
            });
        self.settings_open = open;
    }
}

impl PhotoEnhancer {
    fn update(&mut self, ctx: &egui::Context) {
        let lang = self.preferences.language;
        let colors = self.preferences.theme.palette();
        self.receive(ctx);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("PhotoEnhancer").color(colors.accent));
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(if self.available.is_some() {
                                tr(lang, "Settings · update available")
                            } else {
                                tr(lang, "Settings")
                            })
                            .clicked()
                        {
                            self.settings_open = true;
                        }
                        if ui
                            .add_enabled(
                                !self.busy && !self.installing && self.dialog.is_none(),
                                egui::Button::new(tr(lang, "Open photo…")),
                            )
                            .clicked()
                        {
                            self.dialog(Dialog::Open, ctx);
                        }
                        let description = self
                            .current
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| tr(lang, "Local photo enhancement").into());
                        ui.add(
                            egui::Label::new(RichText::new(description).color(colors.muted))
                                .truncate(),
                        );
                    });
                });
            });
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(self.message.render(lang)).small().color(
                        if self.message_error {
                            colors.warning
                        } else {
                            colors.muted
                        },
                    ))
                    .truncate(),
                )
                .on_hover_text(self.message.render(lang));
            });
        egui::TopBottomPanel::bottom("queue")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| self.batch_ui(ui));
        egui::SidePanel::right("develop")
            .resizable(false)
            .exact_width(360.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| self.controls(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));
        if self.settings_open {
            self.settings_window(ctx);
        }
        // Workers wake the loop on completions and throttled progress changes.
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use std::time::Duration;

    fn frame(
        ctx: &egui::Context,
        app: &mut PhotoEnhancer,
        events: Vec<egui::Event>,
        time: f64,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(920.0, 640.0),
                )),
                events,
                time: Some(time),
                ..Default::default()
            },
            |ctx| app.update(ctx),
        )
    }

    #[test]
    fn dragging_the_comparison_divider_does_not_pan_the_photo() {
        let ctx = egui::Context::default();
        let mut app = PhotoEnhancer::new(
            ctx.clone(),
            Preferences {
                startup_checks: false,
                ..Default::default()
            },
        );
        let image = image::RgbaImage::from_pixel(16, 16, image::Rgba([100, 150, 180, 255]));
        app.before = Some(PhotoEnhancer::texture(&ctx, "before", &image));
        app.after = Some(PhotoEnhancer::texture(&ctx, "after", &image));
        for step in 0..4 {
            let _ = frame(&ctx, &mut app, Vec::new(), step as f64 * 0.1);
        }
        let rect = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("photo_canvas_rect")))
            .unwrap();
        let start = rect.center();
        let end = start + egui::vec2(rect.width() * 0.2, 0.0);
        let _ = frame(&ctx, &mut app, vec![egui::Event::PointerMoved(start)], 0.5);
        let _ = frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            0.6,
        );
        let _ = frame(&ctx, &mut app, vec![egui::Event::PointerMoved(end)], 0.7);
        let _ = frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            0.8,
        );
        assert!(
            (app.split - 0.7).abs() < 0.02,
            "Divider did not track the pointer: {}",
            app.split
        );
        assert_eq!(app.pan, Vec2::ZERO);
    }

    #[test]
    fn export_action_is_visible_at_the_minimum_window_size() {
        let ctx = egui::Context::default();
        let mut app = PhotoEnhancer::new(
            ctx.clone(),
            Preferences {
                startup_checks: false,
                ..Default::default()
            },
        );
        let mut output = egui::FullOutput::default();
        for step in 0..4 {
            output = frame(&ctx, &mut app, Vec::new(), step as f64 * 0.1);
        }
        let found = output.shapes.iter().any(|shape| {
            if let egui::epaint::Shape::Text(text) = &shape.shape
                && text.galley.job.text == "Export image…"
            {
                let bounds = text.galley.rect.translate(text.pos.to_vec2());
                return shape.clip_rect.contains_rect(bounds) && bounds.bottom() <= 640.0;
            }
            false
        });
        assert!(found, "The primary export action was clipped or absent");
    }

    #[test]
    fn failed_open_preserves_the_previous_photo_and_preview() {
        let ctx = egui::Context::default();
        let mut app = PhotoEnhancer::new(
            ctx.clone(),
            Preferences {
                startup_checks: false,
                ..Default::default()
            },
        );
        let original = PathBuf::from("generated-original.png");
        let image = image::RgbaImage::from_pixel(16, 16, image::Rgba([90, 140, 170, 255]));
        app.current = Some(original.clone());
        app.size = Some(image.dimensions());
        app.before = Some(PhotoEnhancer::texture(&ctx, "before", &image));
        app.after = Some(PhotoEnhancer::texture(&ctx, "after", &image));
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send(Selection::Open(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target/nonexistent-gui-fixture.png"),
            ))
            .unwrap();
        app.dialog = Some(receiver);
        app.receive(&ctx);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.busy && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
            app.receive(&ctx);
        }
        assert!(!app.busy && app.message_error);
        assert_eq!(app.current, Some(original));
        assert!(app.before.is_some() && app.after.is_some());
    }

    #[test]
    fn pending_update_waits_for_worker_events_without_periodic_frames() {
        let ctx = egui::Context::default();
        let mut app = PhotoEnhancer::new(
            ctx.clone(),
            Preferences {
                startup_checks: false,
                ..Default::default()
            },
        );
        app.update_busy = true;
        for step in 0..24 {
            let output = frame(&ctx, &mut app, Vec::new(), step as f64 * 0.2);
            if step > 16 {
                assert_eq!(
                    output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
                    Duration::MAX,
                    "Waiting for an updater event must not schedule polling frames"
                );
            }
        }
    }

    #[test]
    fn unchanged_workspace_stops_requesting_frames_after_layout_settles() {
        for (width, height) in [(920.0, 640.0), (1200.0, 820.0), (1280.0, 900.0)] {
            let ctx = egui::Context::default();
            let mut app = PhotoEnhancer::new(
                ctx.clone(),
                Preferences {
                    startup_checks: false,
                    ..Default::default()
                },
            );
            let mut recurring = 0;
            for step in 0..180 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    time: Some(step as f64 / 60.0),
                    ..Default::default()
                };
                let output = ctx.run(input, |ctx| app.update(ctx));
                if step >= 150
                    && output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                        == Duration::ZERO
                {
                    recurring += 1;
                }
            }
            assert_eq!(
                recurring,
                0,
                "Unchanged {width}x{height} UI kept requesting frames: {:?}",
                ctx.repaint_causes()
            );
        }
    }

    #[test]
    fn stationary_hover_over_adjustments_and_scrollbar_does_not_keep_redrawing() {
        for pointer in [
            egui::pos2(1020.0, 160.0),
            egui::pos2(1178.0, 160.0),
            egui::pos2(300.0, 300.0),
        ] {
            let ctx = egui::Context::default();
            let mut app = PhotoEnhancer::new(
                ctx.clone(),
                Preferences {
                    startup_checks: false,
                    ..Default::default()
                },
            );
            let mut recurring = 0;
            for step in 0..240 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 820.0),
                    )),
                    time: Some(step as f64 / 60.0),
                    events: if step == 10 {
                        vec![egui::Event::PointerMoved(pointer)]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                };
                let output = ctx.run(input, |ctx| app.update(ctx));
                if step >= 210
                    && output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                        == Duration::ZERO
                {
                    recurring += 1;
                }
            }
            assert_eq!(
                recurring,
                0,
                "Stationary hover at {pointer:?} kept requesting frames: {:?}",
                ctx.repaint_causes()
            );
        }
    }

    #[test]
    fn settings_with_long_error_remain_inside_the_minimum_window() {
        let ctx = egui::Context::default();
        let preferences = Preferences {
            startup_checks: false,
            ..Default::default()
        };
        let mut app = PhotoEnhancer::new(ctx.clone(), preferences);
        app.settings_open = true;
        app.update_status="Cannot check GitHub releases: the upstream TLS connection failed; please try again later. ".repeat(15).into();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(920.0, 640.0));
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.update(ctx));
        }
        let bounds = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("Settings & updates")))
            .unwrap();
        assert!(
            bounds.height() <= screen.height() && bounds.width() <= screen.width(),
            "Settings escaped the viewport: {bounds:?}"
        );
    }
}
