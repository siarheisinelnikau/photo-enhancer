//! Bounded, deterministic 8-bit sRGB processing. No learned restoration.
use anyhow::{Context, Result, bail, ensure};
use image::{ColorType, DynamicImage, ImageDecoder, ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_INPUT_PIXELS: u64 = 24_000_000;
pub const MAX_OUTPUT_PIXELS: u64 = 32_000_000;
pub const MEMORY_BUDGET: u64 = 512 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const PREVIEW_EDGE: u32 = 1280;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjustments {
    pub exposure: f32,
    pub saturation: f32,
    pub warmth: f32,
    pub denoise: f32,
    pub sharpen: f32,
    pub scale: u32,
}
impl Default for Adjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            saturation: 1.0,
            warmth: 0.0,
            denoise: 0.0,
            sharpen: 0.0,
            scale: 1,
        }
    }
}
impl Adjustments {
    pub fn validate(&self) -> Result<()> {
        for (value, min, max) in [
            (self.exposure, -3.0, 3.0),
            (self.saturation, 0.0, 2.0),
            (self.warmth, -1.0, 1.0),
            (self.denoise, 0.0, 1.0),
            (self.sharpen, 0.0, 2.0),
        ] {
            ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "Adjustment out of range"
            );
        }
        ensure!((1..=4).contains(&self.scale), "Scale must be 1, 2, 3 or 4");
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
}
impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

pub fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled");
    Ok(())
}

pub fn dimensions(width: u32, height: u32, scale: u32) -> Result<(u32, u32)> {
    ensure!(
        width > 0 && height > 0 && (1..=4).contains(&scale),
        "Invalid dimensions or scale"
    );
    let input = u64::from(width) * u64::from(height);
    ensure!(
        input <= MAX_INPUT_PIXELS,
        "Input exceeds the 24 megapixel limit"
    );
    let output = input * u64::from(scale).pow(2);
    ensure!(
        output <= MAX_OUTPUT_PIXELS,
        "Output exceeds 32 megapixels; reduce scale"
    );
    // Includes codec buffers, orientation copies and retained preview textures. This
    // is a conservative allocation plan, not a process-wide allocator guarantee.
    ensure!(
        20 * input + 8 * output + 32 * 1024 * 1024 <= MEMORY_BUDGET,
        "Image exceeds the 512 MiB processing allocation plan; use a smaller image"
    );
    Ok((
        width.checked_mul(scale).context("Width overflow")?,
        height.checked_mul(scale).context("Height overflow")?,
    ))
}

pub fn load(path: &Path, cancel: &AtomicBool) -> Result<RgbaImage> {
    cancelled(cancel)?;
    let mut file = File::open(path).context("Cannot open image")?;
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        "Input file exceeds 128 MiB"
    );
    let mut signature = [0; 8];
    file.read_exact(&mut signature).context("Truncated image")?;
    file.rewind()?;
    if signature == [137, 80, 78, 71, 13, 10, 26, 10] {
        // The generic image decoder does not apply gAMA/cHRM/cICP. Refuse
        // unsupported declarations instead of silently changing the color.
        let decoder = png::Decoder::new_with_limits(
            BufReader::new(&mut file),
            png::Limits {
                bytes: 4 * 1024 * 1024,
            },
        );
        let reader = decoder.read_info()?;
        let info = reader.info();
        ensure!(
            info.animation_control.is_none(),
            "Animated PNG is unsupported; export a still image first"
        );
        if let Some(c) = info.coding_independent_code_points {
            ensure!(
                c.color_primaries == 1
                    && c.transfer_function == 13
                    && c.matrix_coefficients == 0
                    && c.is_video_full_range_image,
                "PNG color space is not sRGB; convert a copy to sRGB first"
            );
        }
        ensure!(
            info.mastering_display_color_volume.is_none() && info.content_light_level.is_none(),
            "HDR PNG is unsupported"
        );
        if info.srgb.is_none() && info.icc_profile.is_none() {
            if let Some(gamma) = info.gama_chunk {
                ensure!(
                    (45454..=45456).contains(&gamma.into_scaled()),
                    "Non-sRGB PNG gamma is unsupported"
                );
            }
            if let Some(chroma) = info.chrm_chunk {
                ensure!(
                    chroma
                        == png::SourceChromaticities::new(
                            (0.3127, 0.3290),
                            (0.64, 0.33),
                            (0.30, 0.60),
                            (0.15, 0.06)
                        ),
                    "Non-sRGB PNG primaries are unsupported"
                );
            }
        }
        drop(reader);
        file.rewind()?;
    } else if signature[..2] == [0xff, 0xd8] {
        validate_jpeg_header(&mut BufReader::new(&mut file), cancel)?;
        file.rewind()?;
    }
    let mut reader = image::ImageReader::new(BufReader::new(file)).with_guessed_format()?;
    ensure!(
        matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)),
        "Only PNG and JPEG are supported in this version"
    );
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(192 * 1024 * 1024);
    limits.max_image_width = Some(24_000);
    limits.max_image_height = Some(24_000);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    dimensions(w, h, 1)?;
    ensure!(
        matches!(
            decoder.color_type(),
            ColorType::L8 | ColorType::La8 | ColorType::Rgb8 | ColorType::Rgba8
        ),
        "Only 8-bit images are supported; convert high-bit-depth images to sRGB first"
    );
    let profile = decoder
        .icc_profile()
        .context("Cannot read the photo's embedded color profile")?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    cancelled(cancel)?;
    if let Some(profile) = profile {
        image = DynamicImage::ImageRgba8(convert_profile_to_srgb(image, &profile, cancel)?);
    }
    image.apply_orientation(orientation);
    Ok(image.into_rgba8())
}

fn convert_profile_to_srgb(
    image: DynamicImage,
    bytes: &[u8],
    cancel: &AtomicBool,
) -> Result<RgbaImage> {
    use moxcms::{ColorProfile, DataColorSpace, Layout, ParsingOptions, TransformOptions};
    cancelled(cancel)?;
    ensure!(
        bytes.len() < 4 * 1024 * 1024,
        "The photo's color profile exceeds the 4 MiB limit"
    );
    let profile = ColorProfile::new_from_slice_with_options(
        bytes,
        ParsingOptions {
            max_profile_size: 4 * 1024 * 1024,
            max_allowed_clut_size: 4 * 1024 * 1024,
            max_allowed_trc_size: 40_000,
        },
    )
    .context("The photo's embedded color profile is damaged or unsupported")?;
    let (layout, channels, alpha) = match image.color() {
        ColorType::Rgb8 => (Layout::Rgb, 3, None),
        ColorType::Rgba8 => (Layout::Rgba, 4, Some(3)),
        ColorType::L8 => (Layout::Gray, 1, None),
        ColorType::La8 => (Layout::GrayAlpha, 2, Some(1)),
        _ => bail!("Color conversion supports 8-bit photos only"),
    };
    ensure!(
        matches!(
            (profile.color_space, layout),
            (DataColorSpace::Rgb, Layout::Rgb | Layout::Rgba)
                | (DataColorSpace::Gray, Layout::Gray | Layout::GrayAlpha)
        ),
        "The embedded color profile does not match this photo's RGB or grayscale pixels"
    );
    let transform = profile
        .create_transform_8bit(
            layout,
            &ColorProfile::new_srgb(),
            Layout::Rgba,
            TransformOptions::default(),
        )
        .context("This photo's color profile cannot be converted to sRGB")?;
    let mut output = RgbaImage::new(image.width(), image.height());
    let input_stride = image.width() as usize * channels;
    let output_stride = image.width() as usize * 4;
    for (src, dst) in image
        .as_bytes()
        .chunks_exact(input_stride)
        .zip(output.as_mut().chunks_exact_mut(output_stride))
    {
        cancelled(cancel)?;
        transform
            .transform(src, dst)
            .context("The photo's color conversion failed")?;
        // Preserve the original alpha exactly, independently of the CMS executor.
        for (source, target) in src.chunks_exact(channels).zip(dst.chunks_exact_mut(4)) {
            target[3] = alpha.map_or(255, |index| source[index]);
        }
    }
    Ok(output)
}

fn validate_jpeg_header(reader: &mut (impl Read + Seek), cancel: &AtomicBool) -> Result<()> {
    reader.seek(SeekFrom::Start(2))?;
    for _ in 0..4096 {
        cancelled(cancel)?;
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        ensure!(byte[0] == 0xff, "Malformed JPEG marker");
        for _ in 0..4096 {
            reader.read_exact(&mut byte)?;
            if byte[0] != 0xff {
                break;
            }
        }
        let marker = byte[0];
        ensure!(
            !matches!(marker, 0xda | 0xd9),
            "JPEG has no supported frame header"
        );
        if matches!(marker, 0xd0..=0xd8 | 0x01) {
            continue;
        }
        let mut length = [0; 2];
        reader.read_exact(&mut length)?;
        let length = u16::from_be_bytes(length);
        ensure!(length >= 2, "Malformed JPEG segment length");
        if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
            ensure!(length >= 8, "Truncated JPEG frame header");
            let mut frame = [0; 6];
            reader.read_exact(&mut frame)?;
            ensure!(
                matches!(frame[5], 1 | 3),
                "CMYK/YCCK JPEG is unsupported; convert a copy to sRGB first"
            );
            ensure!(frame[0] == 8, "Only 8-bit JPEG is supported");
            return Ok(());
        }
        reader.seek(SeekFrom::Current(i64::from(length) - 2))?;
    }
    bail!("JPEG header exceeds the bounded metadata scan")
}

pub fn thumbnail(image: &RgbaImage, cancel: &AtomicBool) -> Result<RgbaImage> {
    cancelled(cancel)?;
    if image.width().max(image.height()) <= PREVIEW_EDGE {
        return Ok(image.clone());
    }
    let ratio = PREVIEW_EDGE as f32 / image.width().max(image.height()) as f32;
    // Premultiplied interpolation avoids dark/colored fringes at transparent edges.
    resize(
        image,
        (image.width() as f32 * ratio).round().max(1.0) as u32,
        (image.height() as f32 * ratio).round().max(1.0) as u32,
        cancel,
        &mut |_| {},
    )
}

fn linear(v: u8) -> f32 {
    let v = v as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

pub fn enhance(
    src: &RgbaImage,
    settings: &Adjustments,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<RgbaImage> {
    settings.validate()?;
    let (ow, oh) = dimensions(src.width(), src.height(), settings.scale)?;
    cancelled(cancel)?;
    let (w, h) = src.dimensions();
    let mut lut = [0.0f32; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = linear(i as u8);
    }
    let exposure = 2.0f32.powf(settings.exposure);
    let gains = [
        1.0 + settings.warmth * 0.18,
        1.0,
        1.0 - settings.warmth * 0.18,
    ];
    let mut filtered = RgbaImage::new(w, h);
    // Fixed 3x3 support and 64-row tiles: no image-sized floating point scratch.
    for top in (0..h).step_by(64) {
        for y in top..(top + 64).min(h) {
            cancelled(cancel)?;
            for x in 0..w {
                let center = src.get_pixel(x, y);
                let mut rgb = [0.0; 3];
                for c in 0..3 {
                    rgb[c] = lut[center[c] as usize];
                }
                if settings.denoise > 0.0 && center[3] > 0 {
                    let mut sum = [0.0; 3];
                    let mut total = 0.0;
                    for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                            let p = src.get_pixel(nx, ny);
                            let distance: f32 =
                                (0..3).map(|c| (lut[p[c] as usize] - rgb[c]).powi(2)).sum();
                            let weight = (p[3] as f32 / 255.0) / (1.0 + distance * 64.0);
                            for c in 0..3 {
                                sum[c] += lut[p[c] as usize] * weight;
                            }
                            total += weight;
                        }
                    }
                    if total > 0.0 {
                        for c in 0..3 {
                            rgb[c] += settings.denoise * (sum[c] / total - rgb[c]);
                        }
                    }
                }
                let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let mut out = *center;
                for c in 0..3 {
                    out[c] = encoded(
                        (luma + (rgb[c] - luma) * settings.saturation) * exposure * gains[c],
                    );
                }
                // Keep hidden RGB as well as alpha unchanged for an identity transform.
                filtered.put_pixel(x, y, out);
            }
            progress(0.4 * (y + 1) as f32 / h as f32);
        }
    }
    if settings.sharpen > 0.0 {
        let mut sharp = RgbaImage::new(w, h);
        for y in 0..h {
            cancelled(cancel)?;
            for x in 0..w {
                let p = filtered.get_pixel(x, y);
                let mut out = *p;
                if p[3] > 0 {
                    let mut sum = [0.0; 3];
                    let mut total = 0.0;
                    for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                            let q = filtered.get_pixel(nx, ny);
                            let weight = q[3] as f32;
                            for c in 0..3 {
                                sum[c] += lut[q[c] as usize] * weight;
                            }
                            total += weight;
                        }
                    }
                    for c in 0..3 {
                        let v = lut[p[c] as usize];
                        out[c] = encoded(v + settings.sharpen * (v - sum[c] / total));
                    }
                }
                sharp.put_pixel(x, y, out);
            }
            progress(0.4 + 0.3 * (y + 1) as f32 / h as f32);
        }
        filtered = sharp;
    }
    if settings.scale > 1 {
        resize(&filtered, ow, oh, cancel, &mut |p| progress(0.7 + 0.3 * p))
    } else {
        progress(1.0);
        Ok(filtered)
    }
}

fn resize(
    src: &RgbaImage,
    w: u32,
    h: u32,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<RgbaImage> {
    let mut dst = RgbaImage::new(w, h);
    let mut lut = [0.0; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = linear(i as u8);
    }
    for y in 0..h {
        cancelled(cancel)?;
        let sy = ((y as f32 + 0.5) * src.height() as f32 / h as f32 - 0.5)
            .clamp(0.0, (src.height() - 1) as f32);
        let y0 = sy.floor() as u32;
        let fy = sy - y0 as f32;
        for x in 0..w {
            let sx = ((x as f32 + 0.5) * src.width() as f32 / w as f32 - 0.5)
                .clamp(0.0, (src.width() - 1) as f32);
            let x0 = sx.floor() as u32;
            let fx = sx - x0 as f32;
            let samples = [
                (x0, y0, (1.0 - fx) * (1.0 - fy)),
                ((x0 + 1).min(src.width() - 1), y0, fx * (1.0 - fy)),
                (x0, (y0 + 1).min(src.height() - 1), (1.0 - fx) * fy),
                (
                    (x0 + 1).min(src.width() - 1),
                    (y0 + 1).min(src.height() - 1),
                    fx * fy,
                ),
            ];
            let mut rgb = [0.0; 3];
            let mut alpha = 0.0;
            for (px, py, weight) in samples {
                let p = src.get_pixel(px, py);
                let a = p[3] as f32 / 255.0 * weight;
                alpha += a;
                for c in 0..3 {
                    rgb[c] += lut[p[c] as usize] * a;
                }
            }
            let mut p = Rgba([0, 0, 0, (alpha * 255.0).round() as u8]);
            if alpha > 0.0 {
                for c in 0..3 {
                    p[c] = encoded(rgb[c] / alpha);
                }
            }
            dst.put_pixel(x, y, p);
        }
        progress((y + 1) as f32 / h as f32);
    }
    Ok(dst)
}

/// Atomic no-clobber publication on the destination filesystem. Never overwrites.
pub fn export(
    image: &RgbaImage,
    path: &Path,
    format: OutputFormat,
    quality: u8,
    cancel: &AtomicBool,
) -> Result<()> {
    cancelled(cancel)?;
    ensure!(
        !path.exists(),
        "Destination already exists; choose a new file name"
    );
    if format == OutputFormat::Jpeg && image.pixels().any(|p| p[3] != 255) {
        bail!("JPEG cannot preserve transparency. Choose PNG for this image.");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .context("Cannot create output in destination folder")?;
    {
        let mut writer = BufWriter::new(temp.as_file_mut());
        match format {
            OutputFormat::Png => {
                let mut encoder = png::Encoder::new(&mut writer, image.width(), image.height());
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
                let mut png = encoder.write_header()?;
                png.write_image_data(image.as_raw())?;
                png.finish()?;
            }
            OutputFormat::Jpeg => {
                let rgb = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
                    let pixel = image.get_pixel(x, y);
                    image::Rgb([pixel[0], pixel[1], pixel[2]])
                });
                image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut writer,
                    quality.clamp(1, 100),
                )
                .encode(
                    rgb.as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
            }
        }
        writer.flush()?;
    }
    temp.as_file().sync_all()?;
    cancelled(cancel)?;
    temp.persist_noclobber(path)
        .map_err(|e| e.error)
        .context("Cannot publish output (existing files are protected)")?;
    Ok(())
}

pub fn output_path(input: &Path, folder: &Path, format: OutputFormat) -> Result<PathBuf> {
    let stem = input.file_stem().context("Input has no file name")?;
    let mut name = stem.to_os_string();
    name.push("-enhanced.");
    name.push(format.extension());
    Ok(folder.join(name))
}
