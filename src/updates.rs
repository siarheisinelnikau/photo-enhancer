//! PhotoEnhancer product configuration; update mechanics live in rust-auto-updator.
pub use rust_auto_updator::{Event, Prepared, Release, Service as Updater, handoff};

pub type Available = Box<Release>;
pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/photo-enhancer";

pub fn config() -> rust_auto_updator::Config {
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "photo-enhancer",
        env!("CARGO_PKG_VERSION"),
        "PhotoEnhancer",
    )
    .expect("valid PhotoEnhancer update configuration");
    config.max_executable_size = 150 * 1024 * 1024;
    config.expected_subsystem = Some(2);
    // Older releases supplied GitHub's digest without a checksum file.
    config.allow_asset_digest = true;
    config.install.stage_prefix = ".photoenhancer-update-".into();
    config.install.helper_name = "PhotoEnhancer.exe".into();
    config.install.startup_health_timeout = Some(std::time::Duration::from_secs(30));
    config
}

pub fn start(wake: impl Fn() + Send + 'static) -> Updater {
    Updater::new(config(), wake).expect("start update worker")
}

/// Process helper arguments before GUI parsing or initialization.
pub fn helper_cli() -> Result<bool, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--apply-update") {
        if args.len() != 1 {
            return Err("Invalid update helper arguments.".into());
        }
        rust_auto_updator::apply_update(&config())?;
        return Ok(true);
    }
    if args.first().is_some_and(|arg| arg == "--cleanup-update") && args.len() != 2 {
        return Err("Invalid update cleanup arguments.".into());
    }
    Ok(false)
}

/// Retain the rollback files until the updated application initializes successfully.
pub fn cleanup_after_startup() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--cleanup-update") && args.len() == 2 {
        rust_auto_updator::cleanup_after_update((&args[1]).into(), config().install);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn product_adapter_accepts_legacy_digest_and_rejects_another_repository() {
        let config = super::config();
        let version = "99.0.0";
        let mut release = serde_json::json!({
            "tag_name": format!("v{version}"),
            "html_url": format!("{}/releases/tag/v{version}",super::REPOSITORY),
            "draft":false,"prerelease":false,"assets":[{
                "name":format!("PhotoEnhancer-v{version}.exe"),"size":10000,
                "browser_download_url":format!("{}/releases/download/v{version}/PhotoEnhancer-v{version}.exe",super::REPOSITORY),
                "digest":format!("sha256:{}","a".repeat(64))
            }]
        });
        assert!(
            rust_auto_updator::parse_release(&release.to_string(), &config)
                .unwrap()
                .is_some()
        );
        release["assets"][0]["browser_download_url"] = "https://github.com/other/photo-enhancer/releases/download/v99.0.0/PhotoEnhancer-v99.0.0.exe".into();
        assert!(rust_auto_updator::parse_release(&release.to_string(), &config).is_err());
    }
}
