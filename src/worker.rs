//! One image worker, one in-flight command; the UI owns the bounded batch queue.
use crate::engine::{self, Adjustments, OutputFormat};
use image::RgbaImage;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};

pub enum Command {
    Open(PathBuf),
    Preview {
        input: PathBuf,
        settings: Adjustments,
    },
    Export {
        input: PathBuf,
        output: PathBuf,
        settings: Adjustments,
        format: OutputFormat,
        quality: u8,
    },
}
pub enum Event {
    Opened {
        path: PathBuf,
        size: (u32, u32),
        before: RgbaImage,
    },
    Previewed(RgbaImage),
    Exported(PathBuf),
    Failed(String),
}
pub struct Worker {
    tx: SyncSender<Command>,
    pub rx: Receiver<Event>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicU32>,
}
impl Worker {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, commands) = mpsc::sync_channel(1);
        let (events, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU32::new(0));
        let stop = cancel.clone();
        let done = progress.clone();
        std::thread::Builder::new()
            .name("image-worker".into())
            .spawn(move || {
                let mut loaded: Option<(PathBuf, RgbaImage)> = None;
                while let Ok(command) = commands.recv() {
                    let mut notified = std::time::Instant::now();
                    let result = (|| -> anyhow::Result<Event> {
                        match command {
                            Command::Open(path) => {
                                // Release the preceding full image before decoding the next one.
                                loaded = None;
                                let image = engine::load(&path, &stop)?;
                                let size = image.dimensions();
                                let before = engine::thumbnail(&image, &stop)?;
                                engine::cancelled(&stop)?;
                                loaded = Some((path.clone(), image));
                                Ok(Event::Opened { path, size, before })
                            }
                            Command::Preview {
                                input,
                                mut settings,
                            } => {
                                if loaded.as_ref().is_none_or(|(path, _)| *path != input) {
                                    loaded = None;
                                    let image = engine::load(&input, &stop)?;
                                    loaded = Some((input, image));
                                }
                                let (_, image) = loaded
                                    .as_ref()
                                    .ok_or_else(|| anyhow::anyhow!("Open an image first"))?;
                                engine::dimensions(image.width(), image.height(), settings.scale)?;
                                let thumb = engine::thumbnail(image, &stop)?;
                                // Preview is a screen-size approximation, not a full resolution cache.
                                settings.scale = 1;
                                let image = engine::enhance(&thumb, &settings, &stop, &mut |p| {
                                    if done.swap((p * 1000.0) as u32, Ordering::Relaxed)
                                        != (p * 1000.0) as u32
                                        && notified.elapsed()
                                            >= std::time::Duration::from_millis(100)
                                    {
                                        wake();
                                        notified = std::time::Instant::now();
                                    }
                                })?;
                                Ok(Event::Previewed(image))
                            }
                            Command::Export {
                                input,
                                output,
                                settings,
                                format,
                                quality,
                            } => {
                                // Batch processing holds one full image at a time. Drop the interactive
                                // cache before loading a different file, retaining only its thumbnail in UI.
                                if loaded.as_ref().is_none_or(|(path, _)| *path != input) {
                                    loaded = None;
                                    let image = engine::load(&input, &stop)?;
                                    loaded = Some((input, image));
                                }
                                let image = &loaded.as_ref().unwrap().1;
                                let result = engine::enhance(image, &settings, &stop, &mut |p| {
                                    if done.swap((p * 900.0) as u32, Ordering::Relaxed)
                                        != (p * 900.0) as u32
                                        && notified.elapsed()
                                            >= std::time::Duration::from_millis(100)
                                    {
                                        wake();
                                        notified = std::time::Instant::now();
                                    }
                                })?;
                                engine::export(&result, &output, format, quality, &stop)?;
                                done.store(1000, Ordering::Relaxed);
                                Ok(Event::Exported(output))
                            }
                        }
                    })();
                    let event = result.unwrap_or_else(|e| Event::Failed(format!("{e:#}")));
                    if events.send(event).is_err() {
                        break;
                    }
                    wake();
                }
            })
            .expect("image worker thread");
        Self {
            tx,
            rx,
            cancel,
            progress,
        }
    }
    pub fn submit(&self, command: Command) -> anyhow::Result<()> {
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.store(0, Ordering::Relaxed);
        self.tx
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("Image worker is busy or unavailable"))
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
