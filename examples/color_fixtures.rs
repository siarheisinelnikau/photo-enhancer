//! Generate reproducible ICC fixtures and normalized results without opening a window.
use anyhow::Result;
use image::ImageEncoder;
use moxcms::ColorProfile;
use photo_enhancer::engine;
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<()> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-output/color-fixtures");
    std::fs::create_dir_all(&folder)?;
    for (name, profile) in [
        ("srgb", ColorProfile::new_srgb()),
        ("display-p3", ColorProfile::new_display_p3()),
        ("adobe-rgb", ColorProfile::new_adobe_rgb()),
    ] {
        let icc = profile.encode()?;
        std::fs::write(folder.join(format!("{name}.icc")), &icc)?;
        let path = folder.join(format!("{name}.png"));
        let mut encoder = image::codecs::png::PngEncoder::new(std::fs::File::create(&path)?);
        encoder.set_icc_profile(icc)?;
        encoder.write_image(
            &[130, 180, 80].repeat(48),
            8,
            6,
            image::ExtendedColorType::Rgb8,
        )?;
        let out = engine::load(&path, &AtomicBool::new(false))?;
        // The generator owns these particular synthetic outputs.
        out.save(folder.join(format!("{name}-normalized.png")))?;
        println!("{name}: {:?}", out.get_pixel(0, 0));
    }
    Ok(())
}
