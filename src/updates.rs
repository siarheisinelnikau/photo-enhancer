//! Public GitHub release checks. No network or installer actions run in tests.
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/photo-enhancer";
const API: &str = "https://api.github.com/repos/siarheisinelnikau/photo-enhancer/releases/latest";
const MAX_DOWNLOAD: u64 = 150 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug)]
pub struct Available {
    pub version: Version,
    pub tag: String,
    pub asset: Asset,
}

pub fn select_release(release: Release, installed: &str) -> Result<Option<Available>> {
    ensure!(
        !release.draft && !release.prerelease,
        "Only stable published releases are supported"
    );
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .context("Release tag is not a semantic version")?;
    ensure!(version.pre.is_empty(), "Prerelease tag on stable channel");
    if version <= Version::parse(installed)? {
        return Ok(None);
    }
    let name = format!("PhotoEnhancer-v{version}.exe");
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == name)
        .context("Release has no compatible portable executable")?;
    let prefix = format!("{REPOSITORY}/releases/download/{}/", release.tag_name);
    ensure!(
        asset.browser_download_url == format!("{prefix}{name}"),
        "Release asset is outside the canonical repository"
    );
    ensure!(
        asset.size > 1024 && asset.size <= MAX_DOWNLOAD,
        "Unexpected update size"
    );
    Ok(Some(Available { version, tag: release.tag_name, asset }))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into()
}
pub fn check() -> Result<Option<Available>> {
    ensure!(
        cfg!(all(target_os = "windows", target_arch = "x86_64")),
        "In-app updates are available for Windows x64 portable builds only"
    );
    let mut response=agent().get(API).header("User-Agent","PhotoEnhancer/0.1.0").header("Accept","application/vnd.github+json").call().context("Cannot check GitHub releases; the repository/release may not be published, or the network/rate limit may be unavailable")?;
    let release: Release = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_json()?;
    select_release(release, env!("CARGO_PKG_VERSION"))
}

pub fn checksum(asset: &Asset) -> Result<[u8; 32]> {
    let value=asset.digest.as_deref().and_then(|d|d.strip_prefix("sha256:")).context("This release has no GitHub SHA-256 digest. In-app installation is unavailable until a verified asset is published.")?;
    ensure!(
        value.len() == 64 && value.is_ascii(),
        "Invalid release digest"
    );
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&value[2 * i..2 * i + 2], 16).context("Invalid release digest")?;
    }
    Ok(bytes)
}

/// Validate a bounded download before handing it to the portable replacement helper.
fn validate_executable(file: &mut std::fs::File, length: u64) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos)?;
    ensure!(
        &dos[..2] == b"MZ",
        "Downloaded asset is not a Windows executable"
    );
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    ensure!(offset <= length.saturating_sub(6), "Invalid PE header");
    file.seek(SeekFrom::Start(offset))?;
    let mut pe = [0; 6];
    file.read_exact(&mut pe)?;
    ensure!(
        &pe[..4] == b"PE\0\0" && pe[4..] == [0x64, 0x86],
        "Update must be a Windows x64 executable"
    );
    Ok(())
}

fn install(update: &Available, cancel: &AtomicBool, progress: &AtomicU32) -> Result<()> {
    ensure!(
        cfg!(all(target_os = "windows", target_arch = "x86_64")),
        "Unsupported platform"
    );
    let expected = checksum(&update.asset)?;
    // Revalidate deserialized metadata, including the URL, immediately before downloading.
    let release = Release {
        tag_name: update.tag.clone(),
        draft: false,
        prerelease: false,
        assets: vec![update.asset.clone()],
    };
    ensure!(
        select_release(release, env!("CARGO_PKG_VERSION"))?.is_some(),
        "Update is no longer newer"
    );
    let mut response = agent()
        .get(&update.asset.browser_download_url)
        .header("User-Agent", "PhotoEnhancer/0.1.0")
        .call()
        .context("Update download failed")?;
    let mut temp = tempfile::NamedTempFile::new()?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD + 1)
        .reader();
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        crate::engine::cancelled(cancel)?;
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        ensure!(
            total <= update.asset.size && total <= MAX_DOWNLOAD,
            "Update download exceeds expected length"
        );
        hash.update(&buffer[..n]);
        temp.write_all(&buffer[..n])?;
        progress.store((total * 950 / update.asset.size) as u32, Ordering::Relaxed);
    }
    ensure!(total == update.asset.size, "Incomplete update download");
    ensure!(
        hash.finalize().as_slice() == expected,
        "Update SHA-256 mismatch; existing executable was not replaced"
    );
    temp.as_file().sync_all()?;
    validate_executable(temp.as_file_mut(), total)?;
    crate::engine::cancelled(cancel)?;
    // The library copies the verified binary beside the running portable EXE and
    // schedules old-file cleanup on Windows; settings live in a separate directory.
    self_replace::self_replace(temp.path())
        .context("Cannot replace the portable executable; check folder write permissions")?;
    progress.store(1000, Ordering::Relaxed);
    Ok(())
}

pub enum Command {
    Check,
    Install(Available),
}
pub enum Event {
    Checked(Result<Option<Available>, String>),
    Installed(Result<(), String>),
}
pub struct Updater {
    tx: SyncSender<Command>,
    pub rx: Receiver<Event>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicU32>,
}
impl Updater {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, commands) = mpsc::sync_channel(1);
        let (events, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU32::new(0));
        let stop = cancel.clone();
        let done = progress.clone();
        std::thread::Builder::new()
            .name("update-worker".into())
            .spawn(move || {
                while let Ok(command) = commands.recv() {
                    let event = match command {
                        Command::Check => Event::Checked(check().map_err(|e| format!("{e:#}"))),
                        Command::Install(update) => Event::Installed(
                            install(&update, &stop, &done).map_err(|e| format!("{e:#}")),
                        ),
                    };
                    if events.send(event).is_err() {
                        break;
                    }
                    wake();
                }
            })
            .expect("update worker thread");
        Self {
            tx,
            rx,
            cancel,
            progress,
        }
    }
    pub fn submit(&self, command: Command) -> Result<()> {
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.store(0, Ordering::Relaxed);
        self.tx
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("Updater is busy or unavailable"))
    }
}
impl Drop for Updater {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> Release {
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            assets: vec![Asset {
                name: format!("PhotoEnhancer-v{version}.exe"),
                browser_download_url: format!(
                    "{REPOSITORY}/releases/download/v{version}/PhotoEnhancer-v{version}.exe"
                ),
                size: 10000,
                digest: Some(format!("sha256:{}", "ab".repeat(32))),
            }],
        }
    }
    #[test]
    fn versions_are_semantic_and_stable() {
        assert!(
            select_release(release("0.10.0"), "0.9.0")
                .unwrap()
                .is_some()
        );
        assert!(select_release(release("0.1.0"), "0.1.0").unwrap().is_none());
        assert!(select_release(release("0.2.0-rc.1"), "0.1.0").is_err());
        let mut r = release("0.2.0");
        r.draft = true;
        assert!(select_release(r, "0.1.0").is_err());
    }
    #[test]
    fn assets_and_digests_are_strict() {
        let mut r = release("0.2.0");
        r.assets[0].browser_download_url = "https://example.org/file.exe".into();
        assert!(select_release(r, "0.1.0").is_err());
        let r = release("0.2.0");
        assert_eq!(checksum(&r.assets[0]).unwrap(), [0xab; 32]);
        let mut a = r.assets[0].clone();
        a.digest = None;
        assert!(checksum(&a).is_err());
        a.digest = Some(format!("sha256:{}", "é".repeat(32)));
        assert!(checksum(&a).is_err());
    }
    #[test]
    fn malformed_executable_is_rejected_without_installing() {
        let mut f = tempfile::tempfile().unwrap();
        f.write_all(&[0; 64]).unwrap();
        assert!(validate_executable(&mut f, 64).is_err());
    }
}
