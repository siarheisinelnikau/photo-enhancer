use image::{ImageEncoder, Rgba, RgbaImage};
use moxcms::ColorProfile;
use photo_enhancer::engine::{self, OutputFormat};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn folder() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
fn png(
    path: &Path,
    profile: Vec<u8>,
    pixels: &[u8],
    width: u32,
    height: u32,
    color: image::ExtendedColorType,
) {
    let mut encoder = image::codecs::png::PngEncoder::new(std::fs::File::create(path).unwrap());
    encoder.set_icc_profile(profile).unwrap();
    encoder.write_image(pixels, width, height, color).unwrap();
}
#[test]
fn srgb_icc_png_opens_preserves_alpha_and_original_bytes() {
    let dir = folder();
    let path = dir.path().join("tagged.png");
    let original = RgbaImage::from_fn(256, 2, |x, y| {
        Rgba([x as u8, 255 - x as u8, 100 + y as u8, x as u8])
    });
    png(
        &path,
        ColorProfile::new_srgb().encode().unwrap(),
        original.as_raw(),
        256,
        2,
        image::ExtendedColorType::Rgba8,
    );
    let bytes = std::fs::read(&path).unwrap();
    let out = engine::load(&path, &AtomicBool::new(false)).unwrap();
    for (a, b) in original.pixels().zip(out.pixels()) {
        assert_eq!(a[3], b[3]);
        for c in 0..3 {
            assert!((a[c] as i16 - b[c] as i16).abs() <= 2);
        }
    }
    assert_eq!(bytes, std::fs::read(path).unwrap());
}
#[test]
fn display_p3_is_converted_instead_of_discarding_the_profile() {
    let dir = folder();
    let path = dir.path().join("p3.png");
    png(
        &path,
        ColorProfile::new_display_p3().encode().unwrap(),
        &[130, 180, 80, 123],
        1,
        1,
        image::ExtendedColorType::Rgba8,
    );
    let out = engine::load(&path, &AtomicBool::new(false)).unwrap();
    let p = out.get_pixel(0, 0);
    assert!(
        p[0] < 125 && p[1] >= 180 && p[2] < 70,
        "Expected P3-to-sRGB gamut conversion: {p:?}"
    );
    assert_eq!(p[3], 123);
    let exported = dir.path().join("normalized.png");
    engine::export(
        &out,
        &exported,
        OutputFormat::Png,
        95,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        engine::load(&exported, &AtomicBool::new(false)).unwrap(),
        out
    );
}
#[test]
fn grayscale_icc_converts_to_neutral_rgb_and_preserves_alpha() {
    let dir = folder();
    let path = dir.path().join("gray.png");
    png(
        &path,
        ColorProfile::new_gray_with_gamma(1.8).encode().unwrap(),
        &[128, 63, 255, 255],
        2,
        1,
        image::ExtendedColorType::La8,
    );
    let out = engine::load(&path, &AtomicBool::new(false)).unwrap();
    let p = out.get_pixel(0, 0);
    assert!(
        (144..=148).contains(&p[0]),
        "Expected gamma conversion: {p:?}"
    );
    assert_eq!(p[0], p[1]);
    assert_eq!(p[1], p[2]);
    assert_eq!(p[3], 63);
}
#[test]
fn adobe_rgb_jpeg_with_icc_opens_with_orientation() {
    let dir = folder();
    let path = dir.path().join("adobe.jpg");
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::fs::File::create(&path).unwrap(),
        95,
    );
    encoder
        .set_icc_profile(ColorProfile::new_adobe_rgb().encode().unwrap())
        .unwrap();
    encoder
        .set_exif_metadata(
            // The encoder adds the APP1 Exif header around this TIFF payload.
            b"II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec(),
        )
        .unwrap();
    encoder
        .encode(
            &[130, 180, 80].repeat(6),
            3,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let out = engine::load(&path, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.dimensions(), (2, 3));
    assert_eq!(out.get_pixel(0, 0)[3], 255);
}
#[test]
fn invalid_or_mismatched_profiles_produce_a_specific_error() {
    let dir = folder();
    let path = dir.path().join("invalid.png");
    png(
        &path,
        b"not an ICC profile".to_vec(),
        &[100, 140, 180],
        1,
        1,
        image::ExtendedColorType::Rgb8,
    );
    let error = engine::load(&path, &AtomicBool::new(false)).unwrap_err();
    assert!(error.to_string().contains("profile"));
    let path = dir.path().join("mismatch.png");
    png(
        &path,
        ColorProfile::new_gray_with_gamma(2.2).encode().unwrap(),
        &[100, 140, 180],
        1,
        1,
        image::ExtendedColorType::Rgb8,
    );
    assert!(
        engine::load(&path, &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("does not match")
    );
}
