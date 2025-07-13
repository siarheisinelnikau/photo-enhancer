#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod host;

use eframe::egui::{self, Color32, RichText, Vec2};
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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([920.0, 640.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let event_loop =
        winit::event_loop::EventLoop::<eframe::UserEvent>::with_user_event().build()?;
    let app = eframe::create_native(
        "PhotoEnhancer",
        options,
        Box::new(|cc| Ok(Box::new(PhotoEnhancer::new(cc)))),
        &event_loop,
    );
    event_loop.run_app(&mut host::ReactiveHost(app))?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
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
    settings_open: bool,
    dialog: Option<Receiver<Selection>>,
    queue: Vec<QueueItem>,
    output_folder: Option<PathBuf>,
    batch_running: bool,
    batch_index: usize,
    update_busy: bool,
    installing: bool,
    update_status: String,
    available: Option<updates::Available>,
    installed: bool,
}
impl PhotoEnhancer {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(29, 106, 111);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(34, 126, 132);
        ctx.set_style(style);
        let preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        let image_ctx = ctx.clone();
        let update_ctx = ctx.clone();
        let worker = Worker::new(move || image_ctx.request_repaint());
        let updater = updates::Updater::new(move || update_ctx.request_repaint());
        let startup = preferences.startup_checks;
        let mut app = Self {
            worker,
            updater,
            preferences,
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
            settings_open: false,
            dialog: None,
            queue: Vec::new(),
            output_folder: None,
            batch_running: false,
            batch_index: 0,
            update_busy: false,
            installing: false,
            update_status: "Not checked".into(),
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
            Ok(()) => self.busy = true,
            Err(e) => self.message = e.to_string(),
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
                    self.current = None;
                    self.size = None;
                    self.before = None;
                    self.after = None;
                    self.preview_settings = None;
                    self.message = "Decoding image…".into();
                    self.command(worker::Command::Open(path));
                }
                Selection::Batch(paths) => {
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
                    Err(error) => self.update_status = error,
                },
                updates::Event::Installed(result) => match result {
                    Ok(()) => {
                        self.installed = true;
                        self.update_status =
                            "Update installed. Restart PhotoEnhancer to use it.".into();
                    }
                    Err(error) => self.update_status = error,
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
            self.update_busy = true;
            self.update_status = "Checking public GitHub releases…".into();
        }
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Develop");
        ui.label(
            RichText::new("DETERMINISTIC · LOCAL CPU")
                .small()
                .color(Color32::from_rgb(110, 207, 192)),
        );
        ui.add_space(8.0);
        ui.add_enabled_ui(!self.busy && !self.installing,|ui| {
            ui.label("Light & color");
            ui.add(egui::Slider::new(&mut self.settings.exposure,-3.0..=3.0).text("Exposure EV"));
            ui.add(egui::Slider::new(&mut self.settings.saturation,0.0..=2.0).text("Saturation"));
            ui.add(egui::Slider::new(&mut self.settings.warmth,-1.0..=1.0).text("Warmth"));
            ui.separator();ui.label("Detail");
            ui.add(egui::Slider::new(&mut self.settings.denoise,0.0..=1.0).text("Denoise"));
            ui.add(egui::Slider::new(&mut self.settings.sharpen,0.0..=2.0).text("Sharpen"));
            ui.label("Edge-aware smoothing · unsharp mask");
            ui.separator();
            egui::ComboBox::from_label("Output size").selected_text(format!("{}×",self.settings.scale)).show_ui(ui,|ui| {
                for scale in 1..=4 {ui.selectable_value(&mut self.settings.scale,scale,format!("{scale}×"));}
            });
            ui.small("Upscaling uses interpolation, not learned restoration.");
            if let Some((w,h))=self.size {match engine::dimensions(w,h,self.settings.scale) {
                Ok((ow,oh))=>{ui.label(format!("{w} × {h}  →  {ow} × {oh}"));}
                Err(e)=>{ui.colored_label(Color32::LIGHT_RED,e.to_string());}
            }}
            if ui.button("Reset adjustments").clicked() {self.settings=Adjustments::default();}
            if ui.add_enabled(self.current.is_some(),egui::Button::new("Render preview").fill(Color32::from_rgb(29,106,111))).clicked() {
                let settings=self.settings.clone();self.pending_preview=Some(settings.clone());
                self.command(worker::Command::Preview {input:self.current.clone().unwrap(),settings});self.message="Rendering screen-size preview…".into();
            }
            if self.after.is_some() && self.preview_settings.as_ref()!=Some(&self.settings) {ui.colored_label(Color32::YELLOW,"Preview needs rendering after your changes.");}
            ui.separator();ui.label("Export");
            egui::ComboBox::from_label("Format").selected_text(self.preferences.format.extension().to_uppercase()).show_ui(ui,|ui| {
                ui.selectable_value(&mut self.preferences.format,OutputFormat::Png,"PNG · lossless / alpha");
                ui.selectable_value(&mut self.preferences.format,OutputFormat::Jpeg,"JPEG · opaque images");
            });
            if self.preferences.format==OutputFormat::Jpeg {ui.add(egui::Slider::new(&mut self.preferences.quality,1..=100).text("JPEG quality"));}
            if ui.add_enabled(self.current.is_some() && self.dialog.is_none(),egui::Button::new("Export image…")).clicked() {
                let filename=engine::output_path(self.current.as_ref().unwrap(),std::path::Path::new(""),self.preferences.format).unwrap().to_string_lossy().into_owned();
                self.dialog(Dialog::Export(filename),ui.ctx());
            }
            ui.small("Originals and existing output files are protected. Metadata is removed. PNG preserves alpha.");
        });
        if self.busy {
            ui.add(
                egui::ProgressBar::new(
                    self.worker.progress.load(Ordering::Relaxed) as f32 / 1000.0,
                )
                .show_percentage(),
            );
            if ui.button("Cancel processing").clicked() {
                self.worker.cancel.store(true, Ordering::Relaxed);
                self.message = "Cancellation requested; waiting for the current codec/row…".into();
            }
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.compare, Compare::Split, "Compare");
            ui.selectable_value(&mut self.compare, Compare::Before, "Before");
            ui.selectable_value(&mut self.compare, Compare::After, "After");
            if ui.button("Fit").clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            ui.add(egui::Slider::new(&mut self.zoom, 0.25..=4.0).text("Preview zoom"));
        });
        if self.compare == Compare::Split {
            ui.add(egui::Slider::new(&mut self.split, 0.0..=1.0).text("Before / after divider"));
        }
        ui.small("Drag to pan · scroll to zoom · preview is limited to a 1280 px edge; exports use the original.");
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            egui::Sense::drag(),
        );
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(16, 21, 26));
        let Some(before) = &self.before else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "A little more light. A little less noise.\n\nOpen an image to begin.",
                egui::FontId::proportional(22.0),
                Color32::from_gray(155),
            );
            return;
        };
        if response.dragged() {
            self.pan += response.drag_delta();
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
                    (1.5, Color32::from_rgb(110, 207, 192)),
                );
            }
        }
    }
    fn batch_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(format!("Batch queue · {} / 64", self.queue.len()));
            ui.add_enabled_ui(
                !self.busy && self.dialog.is_none() && !self.installing,
                |ui| {
                    if ui.button("Add photos…").clicked() {
                        self.dialog(Dialog::AddBatch, ui.ctx());
                    }
                    if ui.button("Output folder…").clicked() {
                        self.dialog(Dialog::Folder, ui.ctx());
                    }
                    if ui
                        .add_enabled(
                            !self.queue.is_empty() && self.output_folder.is_some(),
                            egui::Button::new("Run batch"),
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
                    if ui.button("Clear").clicked() {
                        self.queue.clear();
                    }
                },
            );
        });
        ui.small(self.output_folder.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"Choose a destination. The batch uses the current adjustments and format; name collisions are reported per file.".into()));
        egui::ScrollArea::vertical()
            .max_height(95.0)
            .show(ui, |ui| {
                for item in &self.queue {
                    ui.horizontal(|ui| {
                        ui.label(item.path.file_name().unwrap_or_default().to_string_lossy());
                        ui.label(&item.state);
                    })
                    .response
                    .on_hover_text(item.path.display().to_string());
                }
            });
    }
    fn settings_window(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut open = self.settings_open;
        egui::Window::new("Settings & updates").open(&mut open).default_width(480.0).show(ctx,|ui| {
            ui.heading("PhotoEnhancer");ui.label(format!("Installed version {} · Windows x64 portable",env!("CARGO_PKG_VERSION")));
            ui.hyperlink_to("Canonical repository",updates::REPOSITORY);
            ui.separator();
            if ui.checkbox(&mut self.preferences.startup_checks,"Check for updates on startup").changed() && let Some(storage)=frame.storage_mut() {
                eframe::set_value(storage,"preferences",&self.preferences);storage.flush();
            }
            ui.small("Startup checks contact GitHub only. Images and file paths are never uploaded.");
            if ui.add_enabled(!self.update_busy && !self.installed,egui::Button::new("Check for updates")).clicked() {self.check_updates();}
            ui.label(&self.update_status);
            if let Some(update)=self.available.clone() {
                ui.label(format!("Available: {}",update.version));
                if let Err(error)=updates::checksum(&update.asset) {ui.colored_label(Color32::YELLOW,error.to_string());}
                if ui.add_enabled(!self.update_busy && !self.busy && self.dialog.is_none() && !self.installed && updates::checksum(&update.asset).is_ok(),egui::Button::new("Download and install")).clicked() && self.updater.submit(updates::Command::Install(update)).is_ok() {
                    self.update_busy=true;self.installing=true;self.update_status="Downloading and verifying update…".into();
                }
            }
            if self.installing {
                ui.add(egui::ProgressBar::new(self.updater.progress.load(Ordering::Relaxed) as f32/1000.0).show_percentage());
                if ui.button("Cancel download").clicked() {self.updater.cancel.store(true,Ordering::Relaxed);}
            }
            if self.installed && ui.button("Restart now").clicked() {
                let result=std::env::current_exe().and_then(|exe|std::process::Command::new(exe).spawn());
                match result {Ok(_)=>ctx.send_viewport_cmd(egui::ViewportCommand::Close),Err(e)=>self.update_status=format!("Cannot restart: {e}. Close and reopen the application.")}
            }
            ui.separator();ui.strong("Local model capability");
            if let Capability::Unavailable {reason}=UnavailableBackend.capability() {ui.label(reason);}
            ui.small("An adapter boundary is included for future verified, user-installed models. No model downloads or model setup actions are enabled in this build.");
            ui.separator();ui.label("Supported input: 8-bit, untagged sRGB PNG and JPEG. ICC profiles are rejected. Export removes metadata, including GPS and orientation after applying it.");
            ui.separator();
            egui::CollapsingHeader::new("Licenses & notices").show(ui,|ui| {
                ui.label("PhotoEnhancer © 2026 contributors · MIT License");
                static LINES:std::sync::OnceLock<Vec<&'static str>>=std::sync::OnceLock::new();
                let lines=LINES.get_or_init(||include_str!("../docs/THIRD_PARTY_NOTICES.txt").lines().collect());
                egui::ScrollArea::both().max_height(180.0).show_rows(ui,14.0,lines.len(),|ui,rows| {
                    for row in rows {ui.add(egui::Label::new(RichText::new(lines[row]).monospace().size(11.0)).wrap_mode(egui::TextWrapMode::Extend));}
                });
            });
        });
        self.settings_open = open;
    }
}

impl eframe::App for PhotoEnhancer {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.receive(ctx);
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("PhotoEnhancer").color(Color32::from_rgb(163, 227, 215)));
                ui.label("LOCAL PHOTO WORKSPACE");
                if ui
                    .add_enabled(
                        !self.busy && !self.installing && self.dialog.is_none(),
                        egui::Button::new("Open image…"),
                    )
                    .clicked()
                {
                    self.dialog(Dialog::Open, ctx);
                }
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
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.message);
        });
        egui::TopBottomPanel::bottom("queue")
            .resizable(true)
            .default_height(165.0)
            .height_range(100.0..=260.0)
            .show(ctx, |ui| self.batch_ui(ui));
        egui::SidePanel::right("develop")
            .resizable(false)
            .exact_width(300.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        egui::CentralPanel::default().show(ctx, |ui| self.canvas(ui));
        if self.settings_open {
            self.settings_window(ctx, frame);
        }
        if self.busy || self.update_busy {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "preferences", &self.preferences);
    }
}
