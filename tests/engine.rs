use image::{Rgba, RgbaImage};
use photo_enhancer::engine::*;
use std::sync::atomic::AtomicBool;
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn fixtures() -> tempfile::TempDir {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
    std::fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}

#[test]
fn identity_preserves_all_channels_and_transparency() {
    let image = RgbaImage::from_fn(256, 2, |x, y| {
        Rgba([x as u8, 255 - x as u8, y as u8, x as u8])
    });
    assert_eq!(
        enhance(&image, &Adjustments::default(), &active(), &mut |_| {}).unwrap(),
        image
    );
}
#[test]
fn limits_and_invalid_parameters_are_rejected_before_allocation() {
    assert!(dimensions(u32::MAX, u32::MAX, 4).is_err());
    assert!(dimensions(4000, 3000, 2).is_err());
    assert!(dimensions(0, 1, 1).is_err());
    assert!(dimensions(2000, 1500, 2).is_ok());
    let s = Adjustments {
        exposure: f32::NAN,
        ..Default::default()
    };
    assert!(s.validate().is_err());
}
#[test]
fn cancel_stops_processing_and_export() {
    let image = RgbaImage::new(4, 4);
    let stop = AtomicBool::new(true);
    assert!(enhance(&image, &Adjustments::default(), &stop, &mut |_| {}).is_err());
    assert!(thumbnail(&image, &stop).is_err());
    let folder = fixtures();
    let path = folder.path().join("output.png");
    assert!(export(&image, &path, OutputFormat::Png, 95, &stop).is_err());
    assert!(!path.exists());
}
#[test]
fn denoise_reduces_noise_and_sharpen_increases_edge_contrast() {
    let image = RgbaImage::from_fn(7, 7, |x, y| {
        let v = if (x + y) % 2 == 0 { 120 } else { 136 };
        Rgba([v, v, v, 255])
    });
    let clean = enhance(
        &image,
        &Adjustments {
            denoise: 1.0,
            ..Default::default()
        },
        &active(),
        &mut |_| {},
    )
    .unwrap();
    assert!(
        (clean.get_pixel(3, 3)[0] as i16 - 128).abs()
            < (image.get_pixel(3, 3)[0] as i16 - 128).abs()
    );
    let sharp = enhance(
        &image,
        &Adjustments {
            sharpen: 1.0,
            ..Default::default()
        },
        &active(),
        &mut |_| {},
    )
    .unwrap();
    assert!(sharp.get_pixel(3, 3)[0] < image.get_pixel(3, 3)[0]);
}
#[test]
fn scaling_is_linear_light_and_does_not_bleed_hidden_rgb() {
    let image = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 0])
        }
    });
    let out = enhance(
        &image,
        &Adjustments {
            scale: 2,
            ..Default::default()
        },
        &active(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(out.dimensions(), (4, 2));
    assert_eq!(out.get_pixel(1, 0)[0], 255);
    assert_eq!(out.get_pixel(1, 0)[1], 0);
    assert!(out.get_pixel(1, 0)[3] < 255);
    let ramp = RgbaImage::from_fn(2, 1, |x, _| {
        let v = if x == 0 { 0 } else { 255 };
        Rgba([v, v, v, 255])
    });
    let out = enhance(
        &ramp,
        &Adjustments {
            scale: 2,
            ..Default::default()
        },
        &active(),
        &mut |_| {},
    )
    .unwrap();
    assert!(out.get_pixel(2, 0)[0] > 190);
}
#[test]
fn png_round_trip_preserves_alpha_and_refuses_overwrite() {
    let folder = fixtures();
    let path = folder.path().join("fixture.png");
    let image = RgbaImage::from_fn(4, 3, |x, y| Rgba([x as u8 * 50, y as u8 * 70, 22, 100]));
    export(&image, &path, OutputFormat::Png, 95, &active()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(load(&path, &active()).unwrap(), image);
    assert!(export(&image, &path, OutputFormat::Png, 95, &active()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(
        export(
            &image,
            &folder.path().join("alpha.jpg"),
            OutputFormat::Jpeg,
            95,
            &active()
        )
        .is_err()
    );
}
#[test]
fn jpeg_decode_applies_exif_orientation_once() {
    let folder = fixtures();
    let path = folder.path().join("rotated.jpg");
    let image = RgbaImage::from_pixel(3, 2, Rgba([180, 80, 20, 255]));
    export(&image, &path, OutputFormat::Jpeg, 95, &active()).unwrap();
    let jpeg = std::fs::read(&path).unwrap();
    // Minimal little-endian TIFF, IFD0 Orientation = 6 (90 degrees clockwise).
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut tagged = vec![0xff, 0xd8, 0xff, 0xe1];
    tagged.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    tagged.extend_from_slice(exif);
    tagged.extend_from_slice(&jpeg[2..]);
    std::fs::write(&path, tagged).unwrap();
    assert_eq!(load(&path, &active()).unwrap().dimensions(), (2, 3));
}
#[test]
fn exposure_increases_linear_luminance_without_changing_alpha() {
    let image = RgbaImage::from_pixel(1, 1, Rgba([80, 80, 80, 123]));
    let out = enhance(
        &image,
        &Adjustments {
            exposure: 1.0,
            ..Default::default()
        },
        &active(),
        &mut |_| {},
    )
    .unwrap();
    assert!((110..=115).contains(&out.get_pixel(0, 0)[0]));
    assert_eq!(out.get_pixel(0, 0)[3], 123);
}

#[test]
fn png_color_declarations_and_high_depth_are_not_silently_discarded() {
    let folder = fixtures();
    let path = folder.path().join("linear.png");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = png::Encoder::new(file, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_gamma(png::ScaledFloat::new(1.0));
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[80, 80, 80])
        .unwrap();
    assert!(
        load(&path, &active())
            .unwrap_err()
            .to_string()
            .contains("gamma")
    );
    let path = folder.path().join("high-depth.png");
    image::ImageBuffer::<image::Rgb<u16>, _>::from_pixel(1, 1, image::Rgb([65535, 12345, 0]))
        .save(&path)
        .unwrap();
    assert!(
        load(&path, &active())
            .unwrap_err()
            .to_string()
            .contains("8-bit")
    );
}

#[test]
fn cancellation_during_processing_never_returns_a_partial_result() {
    let image = RgbaImage::from_pixel(20, 20, Rgba([100, 90, 80, 255]));
    let stop = active();
    let result = enhance(&image, &Adjustments::default(), &stop, &mut |p| {
        if p > 0.1 {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert!(result.is_err());
}

#[test]
fn concurrent_exports_cannot_clobber_each_other() {
    let folder = fixtures();
    let path = folder.path().join("race.png");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let jobs: Vec<_> = [12, 240]
        .into_iter()
        .map(|v| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let image = RgbaImage::from_pixel(3, 3, Rgba([v, v, v, 255]));
                barrier.wait();
                export(&image, &path, OutputFormat::Png, 95, &active()).is_ok()
            })
        })
        .collect();
    assert_eq!(
        jobs.into_iter()
            .filter_map(|t| t.join().unwrap().then_some(()))
            .count(),
        1
    );
    let value = load(&path, &active()).unwrap().get_pixel(0, 0)[0];
    assert!(value == 12 || value == 240);
}

#[test]
fn cmyk_jpeg_is_rejected_before_implicit_color_conversion() {
    let folder = fixtures();
    let path = folder.path().join("cmyk.jpg");
    let header = [
        0xff, 0xd8, 0xff, 0xc0, 0, 20, 8, 0, 1, 0, 1, 4, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 4,
        0x11, 0, 0xff, 0xd9,
    ];
    std::fs::write(&path, header).unwrap();
    assert!(
        load(&path, &active())
            .unwrap_err()
            .to_string()
            .contains("CMYK")
    );
}
