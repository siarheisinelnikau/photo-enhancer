use image::{Rgba, RgbaImage};
use photo_enhancer::{
    engine::{self, Adjustments, OutputFormat},
    worker::{Command, Event, Worker},
};
use std::{sync::atomic::AtomicBool, time::Duration};

#[test]
fn worker_recovers_from_failed_jobs_and_reloads_the_selected_preview() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
    std::fs::create_dir_all(&folder).unwrap();
    let folder = tempfile::tempdir_in(folder).unwrap();
    let a = folder.path().join("a.png");
    let b = folder.path().join("b.png");
    for (path, value) in [(&a, 30), (&b, 210)] {
        engine::export(
            &RgbaImage::from_pixel(3, 2, Rgba([value, value, value, 255])),
            path,
            OutputFormat::Png,
            95,
            &AtomicBool::new(false),
        )
        .unwrap();
    }
    let worker = Worker::new(|| {});
    worker
        .submit(Command::Open(folder.path().join("missing.png")))
        .unwrap();
    assert!(matches!(
        worker.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Failed(_)
    ));
    worker.submit(Command::Open(a.clone())).unwrap();
    assert!(matches!(
        worker.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Opened { .. }
    ));
    let output = folder.path().join("b-enhanced.png");
    worker
        .submit(Command::Export {
            input: b,
            output: output.clone(),
            settings: Adjustments::default(),
            format: OutputFormat::Png,
            quality: 95,
        })
        .unwrap();
    assert!(matches!(
        worker.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Exported(_)
    ));
    worker
        .submit(Command::Preview {
            input: a,
            settings: Adjustments::default(),
        })
        .unwrap();
    match worker.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Event::Previewed(image) => assert_eq!(image.get_pixel(0, 0)[0], 30),
        _ => panic!("Expected selected-image preview"),
    }
    assert_eq!(
        engine::load(&output, &AtomicBool::new(false))
            .unwrap()
            .get_pixel(0, 0)[0],
        210
    );
}
