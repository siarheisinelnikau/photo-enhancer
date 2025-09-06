#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod capture;
mod host;
mod preferences;
mod theme;

use egui::{Color32, RichText, Vec2};
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
    time::Duration,
};

fn main() -> anyhow::Result<()> {
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
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            startup_checks: true,
            format: OutputFormat::Png,
            quality: 94,
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
    state: String,
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
    message: String,
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
    update_status: String,
    update_failed: bool,
    available: Option<updates::Available>,
    installed: bool,
}
impl PhotoEnhancer {
    fn new(ctx: egui::Context, preferences: Preferences) -> Self {
        theme::apply(&ctx);
        let image_ctx = ctx.clone();
        let update_ctx = ctx.clone();
        let worker = Worker::new(move || image_ctx.request_repaint());
        let updater = updates::Updater::new(move || update_ctx.request_repaint());
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
            message: "Open a PNG or JPEG to begin. Processing stays on this computer.".into(),
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
            update_status: "Not checked".into(),
            update_failed: false,
            available: None,
            installed: false,
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
                self.message = e.to_string();
                self.message_error = true;
            }
        }
    }
    fn dialog(&mut self, kind: Dialog, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("file-dialog".into())
            .spawn(move || {
                let selected = match kind {
                    Dialog::Open => rfd::FileDialog::new()
                        .add_filter("Photos", &["png", "jpg", "jpeg"])
                        .pick_file()
                        .map(Selection::Open),
                    Dialog::AddBatch => rfd::FileDialog::new()
                        .add_filter("Photos", &["png", "jpg", "jpeg"])
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
                    self.message = "Decoding image…".into();
                    self.command(worker::Command::Open(path));
                }
                Selection::Batch(paths) => {
                    self.batch_open = true;
                    for path in paths {
                        if self.queue.len() == 64 {
                            self.message =
                                "Queue limited to 64 images. Add the rest after this batch.".into();
                            break;
                        }
                        if !self.queue.iter().any(|q| q.path == path) {
                            self.queue.push(QueueItem {
                                path,
                                state: "Waiting".into(),
                            });
                        }
                    }
                }
                Selection::Folder(path) => self.output_folder = Some(path),
                Selection::Export(mut path) => {
                    path.set_extension(self.preferences.format.extension());
                    if let Some(input) = self.current.clone() {
                        self.message = "Exporting full-resolution image…".into();
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
                        "Image ready. Adjust controls, then click Render preview.".into();
                }
                worker::Event::Previewed(image) => {
                    self.after = Some(Self::texture(ctx, "after", &image));
                    self.preview_settings = self.pending_preview.take();
                    self.message =
                        "Preview ready. Export processes the original at full resolution.".into();
                }
                worker::Event::Exported(path) => {
                    self.message = format!("Saved {}", path.display());
                    if self.batch_running && !self.worker.cancel.load(Ordering::Relaxed) {
                        self.queue[self.batch_index].state = "Saved".into();
                        continue_batch = true;
                    } else if self.batch_running {
                        self.queue[self.batch_index].state = "Saved; batch stopped".into();
                        self.batch_running = false;
                    }
                }
                worker::Event::Failed(error) => {
                    self.message_error = true;
                    self.pending_preview = None;
                    self.message = error.clone();
                    if self.batch_running {
                        self.queue[self.batch_index].state = error;
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
        if let Ok(event) = self.updater.rx.try_recv() {
            self.update_busy = false;
            self.installing = false;
            match event {
                updates::Event::Checked(result) => match result {
                    Ok(Some(update)) => {
                        self.update_status = format!("Version {} is available", update.version);
                        self.available = Some(update);
                    }
                    Ok(None) => {
                        self.update_status = "You are up to date".into();
                        self.available = None;
                    }
                    Err(error) => {
                        self.update_status = error;
                        self.update_failed = true;
                    }
                },
                updates::Event::Installed(result) => match result {
                    Ok(()) => {
                        self.installed = true;
                        self.update_status =
                            "Update installed. Restart PhotoEnhancer to use it.".into();
                    }
                    Err(error) => {
                        self.update_status = error;
                        self.update_failed = true;
                    }
                },
            }
        }
    }
    fn next_batch(&mut self) {
        if self.batch_index >= self.queue.len() {
            self.batch_running = false;
            self.message =
                "Batch finished. Review each queue item for saved files or errors.".into();
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
                self.queue[self.batch_index].state = "Processing".into();
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
                self.queue[self.batch_index].state = e.to_string();
                self.batch_running = false;
            }
        }
    }
    fn check_updates(&mut self) {
        if self.updater.submit(updates::Command::Check).is_ok() {
            self.update_failed = false;
            self.update_busy = true;
            self.update_status = "Checking public GitHub releases…".into();
        }
    }
    fn adjustment(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) {
        ui.horizontal(|ui| {
            ui.add_sized(
                [82.0, 30.0],
                egui::Label::new(label).halign(egui::Align::Min),
            );
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
        let enabled = !self.busy && !self.installing && self.dialog.is_none();
        let form_height = (ui.available_height() - 130.0).max(150.0);
        egui::ScrollArea::vertical().id_salt("adjustment_form").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).max_height(form_height).auto_shrink([false,false]).show(ui,|ui| {
            ui.heading("Adjust");
            ui.add_enabled_ui(enabled,|ui| {
                theme::section(ui,"Light & color");
                Self::adjustment(ui,"Exposure",&mut self.settings.exposure,-3.0..=3.0);
                Self::adjustment(ui,"Saturation",&mut self.settings.saturation,0.0..=2.0);
                Self::adjustment(ui,"Warmth",&mut self.settings.warmth,-1.0..=1.0);
                ui.add_space(5.0);ui.separator();
                theme::section(ui,"Detail");
                Self::adjustment(ui,"Denoise",&mut self.settings.denoise,0.0..=1.0);
                Self::adjustment(ui,"Sharpen",&mut self.settings.sharpen,0.0..=2.0);
                ui.add_space(5.0);ui.separator();
                theme::section(ui,"Output");
                egui::ComboBox::from_label("Size").selected_text(format!("{}x enlargement",self.settings.scale)).show_ui(ui,|ui| {
                    for scale in 1..=4 {ui.selectable_value(&mut self.settings.scale,scale,format!("{scale}x"));}
                }).response.on_hover_text("Uses linear-light interpolation. Enlarging does not recover true lost detail.");
                if let Some((w,h))=self.size {match engine::dimensions(w,h,self.settings.scale) {
                    Ok((ow,oh))=>{ui.label(RichText::new(format!("{ow} x {oh} pixels")).color(theme::MUTED));}
                    Err(error)=>{ui.colored_label(theme::WARNING,error.to_string());}
                }}
                egui::ComboBox::from_label("Format").selected_text(match self.preferences.format {OutputFormat::Png=>"PNG · lossless",OutputFormat::Jpeg=>"JPEG"}).show_ui(ui,|ui| {
                    ui.selectable_value(&mut self.preferences.format,OutputFormat::Png,"PNG · lossless / transparency");
                    ui.selectable_value(&mut self.preferences.format,OutputFormat::Jpeg,"JPEG · opaque images");
                });
                if self.preferences.format==OutputFormat::Jpeg {ui.add(egui::Slider::new(&mut self.preferences.quality,1..=100).text("Quality"));}
                ui.add_space(8.0);
                let valid_size=self.size.is_some_and(|(w,h)|engine::dimensions(w,h,self.settings.scale).is_ok());
                if ui.add_enabled(valid_size,egui::Button::new("Update preview").min_size(egui::vec2(ui.available_width(),34.0))).clicked() {
                    let settings=self.settings.clone();self.pending_preview=Some(settings.clone());
                    self.command(worker::Command::Preview {input:self.current.clone().unwrap(),settings});self.message="Updating preview…".into();
                }
                if self.after.is_some() && self.preview_settings.as_ref()!=Some(&self.settings) {ui.colored_label(theme::WARNING,"Adjustments changed. Update the preview.");}
                if ui.button("Reset adjustments").clicked() {self.settings=Adjustments::default();}
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
            if ui.button("Cancel processing").clicked() {
                self.worker.cancel.store(true, Ordering::Relaxed);
                self.message =
                    "Cancellation requested; waiting for the current codec or row…".into();
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
                    .color(theme::MUTED),
                );
            }
            if ui
                .add_enabled(
                    enabled && valid_size,
                    egui::Button::new(RichText::new("Export image…").strong())
                        .fill(theme::ACCENT_DARK)
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
                RichText::new("Creates a new file. Your original stays untouched.")
                    .small()
                    .color(theme::MUTED),
            );
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let has_preview = self.after.is_some();
            ui.add_enabled_ui(has_preview, |ui| {
                ui.selectable_value(&mut self.compare, Compare::Split, "Compare");
            });
            ui.selectable_value(&mut self.compare, Compare::Before, "Before");
            ui.add_enabled_ui(has_preview, |ui| {
                ui.selectable_value(&mut self.compare, Compare::After, "After");
            });
            ui.separator();
            if ui.button("Fit").clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            ui.add(
                egui::Slider::new(&mut self.zoom, 0.25..=4.0)
                    .text("Zoom")
                    .fixed_decimals(2),
            );
        });
        ui.add_space(5.0);
        ui.label(
            RichText::new("Drag to pan · scroll to zoom · drag the divider to compare")
                .small()
                .color(theme::MUTED),
        );
        ui.add_space(5.0);
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            egui::Sense::drag(),
        );
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new("photo_canvas_rect"), rect));
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 6.0, theme::BACKGROUND);
        let Some(before) = &self.before else {
            painter.text(
                rect.center() - Vec2::new(0.0, 28.0),
                egui::Align2::CENTER_CENTER,
                "Start with a photo",
                egui::FontId::proportional(28.0),
                theme::TEXT,
            );
            painter.text(
                rect.center() + Vec2::new(0.0, 15.0),
                egui::Align2::CENTER_CENTER,
                "Open a PNG or JPEG. Everything is processed locally.",
                egui::FontId::proportional(14.0),
                theme::MUTED,
            );
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
                    Color32::from_gray(if (x + y) % 2 == 0 { 40 } else { 49 }),
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
                    (1.5, theme::ACCENT),
                );
            }
        }
        let tag = |position: egui::Pos2, text: &str, align: egui::Align2| {
            let galley = painter.layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(13.0),
                theme::TEXT,
            );
            let anchor = align.anchor_size(position, galley.size() + egui::vec2(20.0, 12.0));
            painter.rect_filled(anchor, 4.0, theme::SURFACE);
            painter.galley(anchor.min + egui::vec2(10.0, 6.0), galley, theme::TEXT);
        };
        if self.compare != Compare::After || self.after.is_none() {
            tag(
                rect.left_top() + Vec2::splat(14.0),
                "BEFORE",
                egui::Align2::LEFT_TOP,
            );
        }
        if self.after.is_some() && self.compare != Compare::Before {
            tag(
                rect.right_top() + Vec2::new(-14.0, 14.0),
                "AFTER",
                egui::Align2::RIGHT_TOP,
            );
        }
    }
    fn batch_ui(&mut self, ui: &mut egui::Ui) {
        let enabled = !self.busy && self.dialog.is_none() && !self.installing;
        ui.horizontal(|ui| {
            ui.toggle_value(
                &mut self.batch_open,
                format!(
                    "Batch export · {} {}",
                    self.queue.len(),
                    if self.queue.len() == 1 {
                        "photo"
                    } else {
                        "photos"
                    }
                ),
            );
            if ui
                .add_enabled(enabled, egui::Button::new("Add photos…"))
                .clicked()
            {
                self.dialog(Dialog::AddBatch, ui.ctx());
            }
            if !self.queue.is_empty()
                && ui
                    .add_enabled(enabled, egui::Button::new("Clear queue"))
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
                .add_enabled(enabled, egui::Button::new("Choose output folder…"))
                .clicked()
            {
                self.dialog(Dialog::Folder, ui.ctx());
            }
            if ui
                .add_enabled(
                    enabled && !self.queue.is_empty() && self.output_folder.is_some(),
                    egui::Button::new("Export batch").fill(theme::ACCENT_DARK),
                )
                .clicked()
            {
                for item in &mut self.queue {
                    item.state = "Waiting".into();
                }
                self.batch_running = true;
                self.batch_index = 0;
                self.next_batch();
            }
            let destination = self
                .output_folder
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Choose where to save the enhanced copies.".into());
            ui.add(egui::Label::new(RichText::new(&destination).color(theme::MUTED)).truncate())
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
                        ui.add(egui::Label::new(&item.state).truncate())
                            .on_hover_text(&item.state);
                    });
                }
            });
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Settings & updates").open(&mut open).default_width(510.0).default_height(580.0).min_width(350.0)
            .default_pos(ctx.content_rect().center()-egui::vec2(265.0,300.0))
            .max_width((ctx.content_rect().width()-48.0).max(350.0))
            .max_height((ctx.content_rect().height()-64.0).max(240.0)).vscroll(true).show(ctx,|ui| {
                ui.heading("PhotoEnhancer");
                ui.label(RichText::new(format!("Version {} · Windows x64",env!("CARGO_PKG_VERSION"))).color(theme::MUTED));
                ui.add_space(6.0);ui.separator();theme::section(ui,"Updates");
                ui.checkbox(&mut self.preferences.startup_checks,"Check for updates on startup");
                ui.label(RichText::new("Only GitHub is contacted. Your photos stay on this computer.").small().color(theme::MUTED));
                if let Some(error)=&self.preferences_error {ui.colored_label(theme::WARNING,error);}
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.update_busy && !self.installed,egui::Button::new("Check for updates")).clicked() {self.check_updates();}
                    if !self.update_failed {ui.add(egui::Label::new(RichText::new(&self.update_status).color(theme::MUTED)).truncate()).on_hover_text(&self.update_status);}
                });
                if self.update_failed {
                    ui.colored_label(theme::WARNING,if self.update_status.contains("404") {"No published release is available yet."} else {"Updates are unavailable. Please try again later."});
                    ui.collapsing("Error details",|ui| {ui.add(egui::Label::new(&self.update_status).wrap());});
                }
                if let Some(update)=self.available.clone() {
                    ui.label(format!("Version {} is available",update.version));
                    if let Err(error)=updates::checksum(&update.asset) {ui.colored_label(theme::WARNING,error.to_string());}
                    if ui.add_enabled(!self.update_busy && !self.busy && self.dialog.is_none() && !self.installed && updates::checksum(&update.asset).is_ok(),egui::Button::new("Download and install").fill(theme::ACCENT_DARK)).clicked() && self.updater.submit(updates::Command::Install(update)).is_ok() {
                        self.update_busy=true;self.installing=true;self.update_failed=false;self.update_status="Downloading and verifying update…".into();
                    }
                }
                if self.installing {
                    ui.add(egui::ProgressBar::new(self.updater.progress.load(Ordering::Relaxed) as f32/1000.0).show_percentage());
                    if ui.button("Cancel download").clicked() {self.updater.cancel.store(true,Ordering::Relaxed);}
                }
                if self.installed && ui.button("Restart now").clicked() {
                    let result=std::env::current_exe().and_then(|exe|std::process::Command::new(exe).spawn());
                    match result {Ok(_)=>ctx.send_viewport_cmd(egui::ViewportCommand::Close),Err(error)=>{self.update_failed=true;self.update_status=format!("Cannot restart: {error}. Close and reopen PhotoEnhancer.");}}
                }
                ui.add_space(6.0);ui.separator();theme::section(ui,"About photo enhancement");
                ui.label("Local filters for light, color, noise and sharpness. Enlargement uses standard interpolation.");
                if let Capability::Unavailable {..}=UnavailableBackend.capability() {ui.label(RichText::new("AI enhancement is not included in this version.").color(theme::MUTED));}
                ui.collapsing("Supported images",|ui| {
                    ui.label("8-bit sRGB PNG and JPEG, up to 24 megapixels. Output is limited to 32 megapixels and the memory budget.");
                    ui.label("ICC, CMYK, HDR and high-bit-depth inputs need conversion first. PNG preserves transparency. Export removes original metadata.");
                });
                ui.hyperlink_to("Project on GitHub",updates::REPOSITORY);
                ui.add_space(6.0);ui.separator();
                ui.collapsing("Rendering diagnostics",|ui| {
                    ui.label("CPU rasterization / Windows GDI");
                    if let Some(metrics)=ctx.data(|data|data.get_temp::<host::FrameMetrics>(egui::Id::new("frame_metrics"))) && metrics.frames>0 {
                        ui.label(format!("Last frame {:.1} ms · average {:.1} ms",metrics.last_ms,metrics.total_ms/metrics.frames as f64));
                        ui.small(format!("Includes UI, rasterization and presentation. Maximum {:.1} ms includes startup.",metrics.max_ms));
                        ui.small(format!("PID {} · CPU time {:.3} s · {} rendered frames",std::process::id(),metrics.process_cpu_seconds,metrics.frames));
                        ui.small(if metrics.graphics_modules.is_empty() {"OpenGL / Direct3D modules: none loaded".into()} else {format!("Detected graphics modules: {}",metrics.graphics_modules.join(", "))});
                    }
                });
                ui.collapsing("Licenses & notices",|ui| {
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
        self.receive(ctx);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("PhotoEnhancer").color(theme::ACCENT));
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(if self.available.is_some() {
                                "Settings · update available"
                            } else {
                                "Settings"
                            })
                            .clicked()
                        {
                            self.settings_open = true;
                        }
                        if ui
                            .add_enabled(
                                !self.busy && !self.installing && self.dialog.is_none(),
                                egui::Button::new("Open photo…"),
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
                            .unwrap_or_else(|| "Local photo enhancement".into());
                        ui.add(
                            egui::Label::new(RichText::new(description).color(theme::MUTED))
                                .truncate(),
                        );
                    });
                });
            });
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&self.message).small().color(
                        if self.message_error {
                            theme::WARNING
                        } else {
                            theme::MUTED
                        },
                    ))
                    .truncate(),
                )
                .on_hover_text(&self.message);
            });
        egui::TopBottomPanel::bottom("queue")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| self.batch_ui(ui));
        egui::SidePanel::right("develop")
            .resizable(false)
            .exact_width(340.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| self.controls(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));
        if self.settings_open {
            self.settings_window(ctx);
        }
        if self.busy || self.update_busy {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;

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
        app.update_status="Cannot check GitHub releases: the upstream TLS connection failed; please try again later. ".repeat(15);
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
