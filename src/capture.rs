//! Developer-only bounded capture, using the actual UI and production CPU renderer.
use crate::{Adjustments, PhotoEnhancer, Preferences, QueueItem, engine};
use anyhow::{Context, Result, bail, ensure};
use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

pub enum Launch {
    Normal,
    Capture(Options),
    Diagnose {
        output: PathBuf,
        input: Option<PathBuf>,
    },
}
pub struct Options {
    pub output: PathBuf,
    pub screen: String,
    pub input: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}
pub fn parse_args() -> Result<Launch> {
    let mut args = std::env::args_os().skip(1);
    let mut capture = None;
    let mut diagnostic = None;
    let mut screen = "workspace".to_owned();
    let mut input = None;
    let mut width = 1280;
    let mut height = 900;
    let mut scale = 1.0f32;
    while let Some(arg) = args.next() {
        let value = args.next().context("Developer arguments require a value")?;
        match arg.to_str().context("Invalid argument")? {
            "--capture" => capture = Some(PathBuf::from(value)),
            "--diagnose" => diagnostic = Some(PathBuf::from(value)),
            "--screen" => {
                screen = value
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Invalid screen"))?
            }
            "--input" => input = Some(PathBuf::from(value)),
            "--width" => width = value.to_str().context("Invalid width")?.parse()?,
            "--height" => height = value.to_str().context("Invalid height")?.parse()?,
            "--scale" => scale = value.to_str().context("Invalid scale")?.parse()?,
            _ => bail!("Unknown developer argument: {}", arg.to_string_lossy()),
        }
    }
    ensure!(
        !(capture.is_some() && diagnostic.is_some()),
        "Choose capture or diagnostic mode"
    );
    if let Some(output) = capture {
        ensure!(output.is_absolute(), "Capture output must be absolute");
        ensure!(
            matches!(
                screen.as_str(),
                "workspace" | "settings" | "empty" | "error"
            ),
            "Unknown capture screen"
        );
        ensure!(
            (920..=2000).contains(&width)
                && (640..=1400).contains(&height)
                && scale.is_finite()
                && (1.0..=1.5).contains(&scale),
            "Capture viewport or scale is outside bounds"
        );
        return Ok(Launch::Capture(Options {
            output,
            screen,
            input,
            width,
            height,
            scale,
        }));
    }
    if let Some(output) = diagnostic {
        ensure!(output.is_absolute(), "Diagnostic output must be absolute");
        return Ok(Launch::Diagnose { output, input });
    }
    ensure!(
        input.is_none() && screen == "workspace" && width == 1280 && height == 900 && scale == 1.0,
        "Screen arguments require --capture or --diagnose"
    );
    Ok(Launch::Normal)
}

pub fn fixture_app(ctx: egui::Context, input: Option<&std::path::Path>) -> Result<PhotoEnhancer> {
    let mut app = PhotoEnhancer::new(
        ctx.clone(),
        Preferences {
            startup_checks: false,
            ..Default::default()
        },
    );
    if let Some(path) = input {
        let stop = AtomicBool::new(false);
        let image = engine::load(path, &stop)?;
        let before = engine::thumbnail(&image, &stop)?;
        app.settings = Adjustments {
            exposure: 0.5,
            saturation: 1.05,
            warmth: 0.05,
            denoise: 0.45,
            sharpen: 0.6,
            scale: 2,
        };
        let mut preview = app.settings.clone();
        preview.scale = 1;
        let after = engine::enhance(&before, &preview, &stop, &mut |_| {})?;
        app.before = Some(PhotoEnhancer::texture(&ctx, "before", &before));
        app.after = Some(PhotoEnhancer::texture(&ctx, "after", &after));
        app.current = Some(path.to_owned());
        app.size = Some(image.dimensions());
        app.preview_settings = Some(app.settings.clone());
        app.message =
            "Preview ready. Export creates a new file and keeps your original untouched.".into();
        app.queue.push(QueueItem {
            path: path.to_owned(),
            state: "Waiting".into(),
        });
    }
    Ok(app)
}

pub fn save(options: Options) -> Result<()> {
    let started = Instant::now();
    ensure!(
        !options.output.exists(),
        "Capture output already exists; use a fresh path"
    );
    if matches!(options.screen.as_str(), "workspace" | "settings") {
        ensure!(
            options.input.is_some(),
            "Populated captures require an input fixture"
        );
    }
    let ctx = egui::Context::default();
    let populated = matches!(options.screen.as_str(), "workspace" | "settings");
    let mut app = fixture_app(
        ctx.clone(),
        if populated {
            options.input.as_deref()
        } else {
            None
        },
    )?;
    app.settings_open = options.screen == "settings";
    if options.screen == "error" {
        let path = options
            .input
            .as_deref()
            .context("Error capture requires an invalid fixture")?;
        app.message = engine::load(path, &AtomicBool::new(false))
            .err()
            .context("Error fixture unexpectedly decoded")?
            .to_string()
            .into();
        app.message_error = true;
    }
    let width = (options.width as f32 * options.scale).round() as u32;
    let height = (options.height as f32 * options.scale).round() as u32;
    let mut pixels = vec![0u32; (width * height) as usize];
    let mut renderer = EguiSoftwareRender::new(ColorFieldOrder::Bgra);
    for frame in 0..16 {
        ensure!(
            started.elapsed().as_secs() < 10,
            "Self-capture exceeded its internal deadline"
        );
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(options.width as f32, options.height as f32),
            )),
            time: Some(frame as f64 / 30.0),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(options.scale);
        let output = ctx.run(input, |ctx| app.update(ctx));
        crate::host::rasterize(&ctx, &mut renderer, &mut pixels, width, height, output);
    }
    let rgba = image::RgbaImage::from_fn(width, height, |x, y| {
        let color = pixels[(y * width + x) as usize].to_le_bytes();
        image::Rgba([color[2], color[1], color[0], 255])
    });
    let parent = options
        .output
        .parent()
        .context("Capture output has no parent")?;
    std::fs::create_dir_all(parent)?;
    engine::export(
        &rgba,
        &options.output,
        engine::OutputFormat::Png,
        95,
        &AtomicBool::new(false),
    )?;
    let manifest = serde_json::json!({"executable":std::env::current_exe()?,"pid":std::process::id(),"backend":"egui_software_backend 0.0.2 CPU; production UI and rasterizer; offscreen client area","screen":options.screen,"logical_width":options.width,"logical_height":options.height,"scale":options.scale,"physical_width":width,"physical_height":height,"frames":16,"elapsed_ms":started.elapsed().as_millis(),"settings_persisted":false,"network_requests":0,"graphics_modules":crate::host::graphics_modules()});
    std::fs::write(
        options.output.with_extension("json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
