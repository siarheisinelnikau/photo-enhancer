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

    #[test]
    fn old_json_keeps_export_options_and_defaults_new_preferences() {
        let preferences: Preferences =
            serde_json::from_str(r#"{"startup_checks":false,"format":"Jpeg","quality":87}"#)
                .unwrap();
        assert!(!preferences.startup_checks);
        assert_eq!(preferences.format, crate::OutputFormat::Jpeg);
        assert_eq!(preferences.quality, 87);
        assert_eq!(preferences.language, crate::Language::English);
        assert_eq!(preferences.theme, crate::theme::Theme::Dark);
        let future: Preferences =
            serde_json::from_str(r#"{"language":"future-language","theme":"future-theme"}"#)
                .unwrap();
        assert_eq!(future.language, crate::Language::English);
        assert_eq!(future.theme, crate::theme::Theme::Dark);
    }

    #[test]
    fn language_and_theme_survive_atomic_save_without_resetting_opt_out() {
        let directory = tempfile::tempdir().unwrap();
        let store = PreferencesStore {
            path: directory.path().join("preferences.json"),
        };
        let preferences = Preferences {
            startup_checks: false,
            language: crate::Language::Russian,
            theme: crate::theme::Theme::Light,
            ..Default::default()
        };
        store.save(&preferences).unwrap();
        assert_eq!(store.load().unwrap(), preferences);
    }
}
