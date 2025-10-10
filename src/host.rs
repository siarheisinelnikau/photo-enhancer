//! Reactive egui host: CPU rasterization, softbuffer Win32 GDI presentation.
use crate::{PhotoEnhancer, Preferences, preferences::PreferencesStore};
use anyhow::{Context, Result};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::{num::NonZeroU32, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    window::{Window, WindowId},
};

#[derive(Debug)]
enum Wake {
    Repaint { when: Instant, pass: u64 },
}

struct Host {
    // Surface is dropped before the context and owning window.
    surface: Option<softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>>,
    display: Option<softbuffer::Context<OwnedDisplayHandle>>,
    window: Option<Arc<Window>>,
    bridge: Option<egui_winit::State>,
    ctx: egui::Context,
    app: PhotoEnhancer,
    renderer: EguiSoftwareRender,
    pixels: Vec<u32>,
    last_size: (u32, u32),
    dirty: bool,
    deadline: Option<(Instant, u64)>,
    close: bool,
    store: Option<PreferencesStore>,
    saved: Preferences,
    error: Option<String>,
    metrics: FrameMetrics,
    diagnostic: Option<Diagnostic>,
}

#[derive(Clone, Default)]
pub struct FrameMetrics {
    pub frames: u64,
    pub last_ms: f64,
    pub max_ms: f64,
    pub total_ms: f64,
    pub process_cpu_seconds: f64,
    pub graphics_modules: Vec<String>,
    pub presents: u64,
    pub events: std::collections::BTreeMap<String, u64>,
    pub repaint_causes: std::collections::BTreeMap<String, u64>,
}

pub fn run(diagnostic: Option<(std::path::PathBuf, Option<std::path::PathBuf>)>) -> Result<()> {
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    let ctx = egui::Context::default();
    let proxy = event_loop.create_proxy();
    ctx.set_request_repaint_callback(move |info| {
        if let Some(when) = Instant::now().checked_add(info.delay) {
            let _ = proxy.send_event(Wake::Repaint {
                when,
                pass: info.current_cumulative_pass_nr,
            });
        }
    });
    let (store, prefs, error) = if diagnostic.is_some() {
        (
            None,
            Preferences {
                startup_checks: false,
                ..Default::default()
            },
            None,
        )
    } else {
        match PreferencesStore::default_path() {
            Ok(store) => match store.load() {
                Ok(prefs) => (Some(store), prefs, None),
                Err(error) => (
                    Some(store),
                    Preferences::default(),
                    Some(format!("{error:#}")),
                ),
            },
            Err(error) => (None, Preferences::default(), Some(format!("{error:#}"))),
        }
    };
    let mut app = if let Some((_, input)) = &diagnostic {
        crate::capture::fixture_app(ctx.clone(), input.as_deref())?
    } else {
        PhotoEnhancer::new(ctx.clone(), prefs.clone())
    };
    app.preferences_error = error;
    let mut host = Host {
        surface: None,
        display: None,
        window: None,
        bridge: None,
        ctx,
        app,
        renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
        pixels: Vec::new(),
        last_size: (0, 0),
        dirty: true,
        deadline: None,
        close: false,
        store,
        saved: prefs,
        error: None,
        metrics: FrameMetrics::default(),
        diagnostic: diagnostic.map(|(output, _)| Diagnostic::new(output)),
    };
    event_loop.run_app(&mut host)?;
    if let Some(error) = host.error {
        anyhow::bail!(error);
    }
    Ok(())
}

impl Host {
    fn diagnostic_step(&mut self, event_loop: &ActiveEventLoop) {
        let window_visible = self.visible();
        let Some(diagnostic) = &mut self.diagnostic else {
            return;
        };
        if self.window.is_none() || Instant::now() < diagnostic.next {
            return;
        }
        let now = Instant::now();
        match diagnostic.phase {
            0 => {
                diagnostic.baseline = Some(Sample::new(&self.metrics));
                diagnostic.next = now + std::time::Duration::from_secs(5);
            }
            1 => {
                diagnostic.finish_sample("visible_settled", &self.metrics, window_visible);
                self.window.as_ref().unwrap().set_visible(false);
                diagnostic.baseline = Some(Sample::new(&self.metrics));
                diagnostic.next = now + std::time::Duration::from_secs(5);
            }
            2 => {
                diagnostic.finish_sample("hidden", &self.metrics, window_visible);
                self.window.as_ref().unwrap().set_visible(true);
                self.dirty = true;
                diagnostic.next = now + std::time::Duration::from_secs(1);
            }
            3 => {
                diagnostic.baseline = Some(Sample::new(&self.metrics));
                diagnostic.next = now + std::time::Duration::from_secs(3);
            }
            4 => {
                diagnostic.finish_sample("restored_settled", &self.metrics, window_visible);
                let report = serde_json::json!({"executable":std::env::current_exe().ok(),"pid":std::process::id(),"backend":"egui CPU rasterization; softbuffer Win32 GDI","graphics_modules":graphics_modules(),"logical_cpu_count":std::thread::available_parallelism().map(|n|n.get()).unwrap_or(1),"samples":diagnostic.samples,"total_layout_and_raster_frames":self.metrics.frames,"total_presentations":self.metrics.presents,"last_frame_ms":self.metrics.last_ms,"max_frame_ms":self.metrics.max_ms,"average_frame_ms":self.metrics.total_ms/self.metrics.frames.max(1) as f64,"elapsed_seconds":diagnostic.started.elapsed().as_secs_f64(),"network_requests":0,"settings_persisted":false,"interactive_input_tested":false});
                let result = (|| -> Result<()> {
                    let parent = diagnostic
                        .output
                        .parent()
                        .context("Diagnostic output has no parent")?;
                    std::fs::create_dir_all(parent)?;
                    std::fs::write(&diagnostic.output, serde_json::to_vec_pretty(&report)?)?;
                    Ok(())
                })();
                if let Err(error) = result {
                    self.error = Some(format!("{error:#}"));
                }
                self.close = true;
                event_loop.exit();
            }
            _ => {}
        }
        diagnostic.phase += 1;
    }
    fn visible(&self) -> bool {
        self.window.as_ref().is_some_and(|window| {
            window.is_visible() != Some(false)
                && window.is_minimized() != Some(true)
                && window.inner_size().width > 0
                && window.inner_size().height > 0
        })
    }
    fn persist(&mut self) {
        if self.saved != self.app.preferences
            && let Some(store) = &self.store
        {
            match store.save(&self.app.preferences) {
                Ok(()) => {
                    self.saved = self.app.preferences.clone();
                    self.app.preferences_error = None;
                }
                Err(error) => {
                    self.app.preferences_error = Some(format!("{error:#}"));
                    self.saved = self.app.preferences.clone();
                }
            }
        }
    }
    fn present(&mut self) -> Result<()> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let surface = self.surface.as_mut().context("No software surface")?;
        if self.last_size != (size.width, size.height) {
            surface
                .resize(width, height)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            self.last_size = (size.width, size.height);
            self.dirty = true;
        }
        let start = Instant::now();
        let rendered = self.dirty || self.pixels.is_empty();
        if rendered {
            let bridge = self.bridge.as_mut().unwrap();
            egui_winit::update_viewport_info(
                bridge
                    .egui_input_mut()
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default(),
                &self.ctx,
                window,
                false,
            );
            let input = bridge.take_egui_input(window);
            self.metrics.process_cpu_seconds = process_cpu_seconds().unwrap_or_default();
            self.metrics.graphics_modules = graphics_modules();
            self.ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("frame_metrics"), self.metrics.clone())
            });
            let mut output = self.ctx.run(input, |ctx| self.app.update(ctx));
            if self.diagnostic.is_some() {
                for cause in self.ctx.repaint_causes() {
                    let file = cause.file.rsplit(['/', '\\']).next().unwrap_or(cause.file);
                    let key = format!("{file}:{} {}", cause.line, cause.reason);
                    if self.metrics.repaint_causes.len() < 32
                        || self.metrics.repaint_causes.contains_key(&key)
                    {
                        *self.metrics.repaint_causes.entry(key).or_default() += 1;
                    }
                }
            }
            bridge.handle_platform_output(window, std::mem::take(&mut output.platform_output));
            let mut actions = Vec::new();
            if let Some(root) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                self.close = root
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close));
                egui_winit::process_viewport_commands(
                    &self.ctx,
                    bridge
                        .egui_input_mut()
                        .viewports
                        .entry(egui::ViewportId::ROOT)
                        .or_default(),
                    root.commands.clone(),
                    window,
                    &mut actions,
                );
            }
            let length = (size.width as usize)
                .checked_mul(size.height as usize)
                .context("Framebuffer overflow")?;
            anyhow::ensure!(
                length <= 32_000_000,
                "Window framebuffer exceeds the size limit"
            );
            self.pixels.resize(length, 0);
            rasterize(
                &self.ctx,
                &mut self.renderer,
                &mut self.pixels,
                size.width,
                size.height,
                output,
            );
            self.dirty = false;
        }
        let mut buffer = surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        buffer.copy_from_slice(&self.pixels);
        buffer
            .present()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        self.metrics.presents += 1;
        if rendered {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            self.metrics.frames += 1;
            self.metrics.last_ms = elapsed;
            self.metrics.max_ms = self.metrics.max_ms.max(elapsed);
            self.metrics.total_ms += elapsed;
        }
        self.persist();
        Ok(())
    }
    fn schedule(&mut self, event_loop: &ActiveEventLoop) {
        if self
            .deadline
            .is_some_and(|(_, pass)| pass + 1 < self.ctx.cumulative_pass_nr())
        {
            self.deadline = None;
        }
        if let Some((when, _)) = self.deadline
            && when <= Instant::now()
        {
            self.deadline = None;
            self.dirty = true;
        }
        if self.dirty && self.visible() {
            self.window.as_ref().unwrap().request_redraw();
        }
        let ui_flow = if self.visible() {
            self.deadline
                .map_or(ControlFlow::Wait, |(when, _)| ControlFlow::WaitUntil(when))
        } else {
            ControlFlow::Wait
        };
        let flow = if let Some(diagnostic) = &self.diagnostic {
            match ui_flow {
                ControlFlow::WaitUntil(when) => ControlFlow::WaitUntil(when.min(diagnostic.next)),
                _ => ControlFlow::WaitUntil(diagnostic.next),
            }
        } else {
            ui_flow
        };
        event_loop.set_control_flow(flow);
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

pub(crate) fn rasterize(
    ctx: &egui::Context,
    renderer: &mut EguiSoftwareRender,
    pixels: &mut [u32],
    width: u32,
    height: u32,
    output: egui::FullOutput,
) {
    let shapes = ctx.tessellate(output.shapes, output.pixels_per_point);
    pixels.fill(0x0010151a);
    renderer.render(
        &mut BufferMutRef::new(
            bytemuck::cast_slice_mut(pixels),
            width as usize,
            height as usize,
        ),
        &shapes,
        &output.textures_delta,
        output.pixels_per_point,
    );
}

struct Sample {
    at: Instant,
    cpu: Option<f64>,
    frames: u64,
    presents: u64,
    events: std::collections::BTreeMap<String, u64>,
    repaint_causes: std::collections::BTreeMap<String, u64>,
}
impl Sample {
    fn new(metrics: &FrameMetrics) -> Self {
        Self {
            at: Instant::now(),
            cpu: process_cpu_seconds(),
            frames: metrics.frames,
            presents: metrics.presents,
            events: metrics.events.clone(),
            repaint_causes: metrics.repaint_causes.clone(),
        }
    }
}
struct Diagnostic {
    output: std::path::PathBuf,
    started: Instant,
    next: Instant,
    phase: u32,
    baseline: Option<Sample>,
    samples: Vec<serde_json::Value>,
}
impl Diagnostic {
    fn new(output: std::path::PathBuf) -> Self {
        let now = Instant::now();
        Self {
            output,
            started: now,
            next: now + std::time::Duration::from_secs(2),
            phase: 0,
            baseline: None,
            samples: Vec::new(),
        }
    }
    fn finish_sample(&mut self, state: &str, metrics: &FrameMetrics, window_visible: bool) {
        if let Some(start) = self.baseline.take() {
            let wall = start.at.elapsed().as_secs_f64();
            let cpu = process_cpu_seconds()
                .zip(start.cpu)
                .map(|(end, start)| end - start);
            let difference =
                |after: &std::collections::BTreeMap<String, u64>,
                 before: &std::collections::BTreeMap<String, u64>| {
                    after
                        .iter()
                        .filter_map(|(key, value)| {
                            let delta = value - before.get(key).copied().unwrap_or(0);
                            (delta > 0).then_some((key.clone(), delta))
                        })
                        .collect::<std::collections::BTreeMap<_, _>>()
                };
            self.samples.push(serde_json::json!({"state":state,"window_visible":window_visible,"cpu_available":cpu.is_some(),"wall_seconds":wall,"cpu_seconds":cpu,"cpu_percent_one_core":cpu.map(|cpu|100.0*cpu/wall),"cpu_percent_machine":cpu.map(|cpu|100.0*cpu/wall/std::thread::available_parallelism().map(|n|n.get()).unwrap_or(1) as f64),"layout_frames":metrics.frames-start.frames,"raster_frames":metrics.frames-start.frames,"presentations":metrics.presents-start.presents,"events":difference(&metrics.events,&start.events),"repaint_causes":difference(&metrics.repaint_causes,&start.repaint_causes)}));
        }
    }
}

#[cfg(windows)]
fn process_cpu_seconds() -> Option<f64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: each output pointer names initialized, writable storage; the pseudo
    // handle belongs to this process and does not need to be closed.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return None;
    }
    let ticks = |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    Some((ticks(kernel) + ticks(user)) as f64 / 10_000_000.0)
}
#[cfg(not(windows))]
fn process_cpu_seconds() -> Option<f64> {
    None
}

#[cfg(windows)]
pub(crate) fn graphics_modules() -> Vec<String> {
    [
        "opengl32.dll",
        "libEGL.dll",
        "libGLESv2.dll",
        "d3d11.dll",
        "d3d12.dll",
        "dxgi.dll",
    ]
    .into_iter()
    .filter(|name| {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: the name is a valid NUL-terminated string. This queries only
        // loaded modules of this process and never loads a DLL.
        !unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(wide.as_ptr()) }
            .is_null()
    })
    .map(str::to_owned)
    .collect()
}
#[cfg(not(windows))]
pub(crate) fn graphics_modules() -> Vec<String> {
    Vec::new()
}

impl ApplicationHandler<Wake> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            let icon =
                image::load_from_memory(include_bytes!("../assets/app-icon.png"))?.into_rgba8();
            let (icon_width, icon_height) = icon.dimensions();
            let icon = winit::window::Icon::from_rgba(icon.into_raw(), icon_width, icon_height)?;
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_window_icon(Some(icon))
                        .with_title(if self.diagnostic.is_some() {
                            "PhotoEnhancer [bounded diagnostic]"
                        } else {
                            "PhotoEnhancer"
                        })
                        .with_active(self.diagnostic.is_none())
                        .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 820.0))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(920.0, 640.0))
                        .with_theme(Some(winit::window::Theme::Dark)),
                )?,
            );
            let context = softbuffer::Context::new(event_loop.owned_display_handle())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let surface = softbuffer::Surface::new(&context, window.clone())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            self.bridge = Some(egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                window.as_ref(),
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(4096),
            ));
            self.surface = Some(surface);
            self.display = Some(context);
            self.window = Some(window);
            self.dirty = true;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        if self.diagnostic.is_some() {
            *self
                .metrics
                .events
                .entry("egui_repaint".into())
                .or_default() += 1;
        }
        self.app.receive(&self.ctx);
        let Wake::Repaint { when, pass } = event;
        if pass + 1 >= self.ctx.cumulative_pass_nr()
            && self.deadline.is_none_or(|(pending, _)| when < pending)
        {
            self.deadline = Some((when, pass));
        }
        self.schedule(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        self.diagnostic_step(event_loop);
        self.schedule(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if id != window.id() {
            return;
        }
        if self.diagnostic.is_some() {
            let name = match &event {
                WindowEvent::CursorMoved { .. } => "pointer_move",
                WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                    "pointer_boundary"
                }
                WindowEvent::MouseInput { .. } => "pointer_button",
                WindowEvent::MouseWheel { .. } => "wheel",
                WindowEvent::KeyboardInput { .. } => "key",
                WindowEvent::Focused(_) => "focus",
                WindowEvent::Resized(_) => "resize",
                WindowEvent::ScaleFactorChanged { .. } => "dpi",
                WindowEvent::Occluded(_) => "occlusion",
                WindowEvent::RedrawRequested => "native_redraw",
                _ => "other",
            };
            *self.metrics.events.entry(name.into()).or_default() += 1;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.persist();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if self.visible()
                    && let Err(error) = self.present()
                {
                    self.fail(event_loop, error);
                }
            }
            _ => {
                let response = self
                    .bridge
                    .as_mut()
                    .unwrap()
                    .on_window_event(window, &event);
                if response.repaint
                    || matches!(
                        event,
                        WindowEvent::Resized(_)
                            | WindowEvent::ScaleFactorChanged { .. }
                            | WindowEvent::Occluded(false)
                    )
                {
                    self.dirty = true;
                }
            }
        }
        if self.close {
            self.persist();
            event_loop.exit();
        }
        self.schedule(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.schedule(event_loop);
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.persist();
        self.ctx.set_request_repaint_callback(|_| {});
        self.surface = None;
        self.display = None;
        self.bridge = None;
        self.window = None;
    }
}
