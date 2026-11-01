use crate::Preferences;
use anyhow::{Context, Result, ensure};
use std::{collections::HashMap, io::Write, path::PathBuf};

pub struct PreferencesStore {
    path: PathBuf,
}
impl PreferencesStore {
    pub fn default_path() -> Result<Self> {
        let directory = dirs::data_dir()
            .context("Application-data folder is unavailable")?
            .join("PhotoEnhancer/data");
        Ok(Self {
            path: directory.join("preferences.json"),
        })
    }
    pub fn load(&self) -> Result<Preferences> {
        if self.path.exists() {
            ensure!(
                self.path.metadata()?.len() <= 64 * 1024,
                "Settings file exceeds the size limit"
            );
            return serde_json::from_slice(&std::fs::read(&self.path)?)
                .context("Cannot read preferences");
        }
        // Preserve the former eframe file, migrating only our own preferences.
        let legacy = self.path.with_file_name("app.ron");
        if legacy.exists() {
            ensure!(
                legacy.metadata()?.len() <= 1024 * 1024,
                "Legacy settings exceed the size limit"
            );
            let values: HashMap<String, String> = ron::from_str(&std::fs::read_to_string(legacy)?)?;
            if let Some(value) = values.get("preferences") {
                return ron::from_str(value).context("Cannot read legacy preferences");
            }
        }
        Ok(Preferences::default())
    }
    pub fn save(&self, preferences: &Preferences) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("Settings have no parent folder")?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, preferences)?;
        file.write_all(b"\n")?;
        file.as_file().sync_all()?;
        file.persist(&self.path)
            .map_err(|error| error.error)
            .context("Cannot save preferences")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrate_startup_opt_out_and_format_without_changing_legacy_file() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
        std::fs::create_dir_all(&root).unwrap();
        let directory = tempfile::tempdir_in(root).unwrap();
        let store = PreferencesStore {
            path: directory.path().join("preferences.json"),
        };
        let prefs = Preferences {
            startup_checks: false,
            format: crate::OutputFormat::Jpeg,
            quality: 87,
            ..Default::default()
        };
        let old = ron::to_string(&HashMap::from([(
            "preferences",
            ron::to_string(&prefs).unwrap(),
        )]))
        .unwrap();
        let legacy = directory.path().join("app.ron");
        std::fs::write(&legacy, &old).unwrap();
        let migrated = store.load().unwrap();
        assert_eq!(migrated, prefs);
        store.save(&migrated).unwrap();
        assert_eq!(store.load().unwrap(), prefs);
        assert_eq!(std::fs::read_to_string(legacy).unwrap(), old);
    }
}
