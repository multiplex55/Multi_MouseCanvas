use super::model::AppSettings;
use directories::ProjectDirs;
use std::{fs, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("could not determine the application settings directory")]
    MissingSettingsDirectory,
    #[error("failed to read settings from {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write settings to {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse settings from {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to serialize settings: {0}")]
    Serialize(serde_json::Error),
}

pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    let dirs = ProjectDirs::from("com", "MultiMouseCanvas", "MultiMouseCanvas")
        .ok_or(SettingsError::MissingSettingsDirectory)?;
    Ok(dirs.config_dir().join("settings.json"))
}

pub fn load_or_default(path: &PathBuf) -> Result<AppSettings, SettingsError> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let text = fs::read_to_string(path).map_err(|source| SettingsError::Read {
        path: path.clone(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| SettingsError::Parse {
        path: path.clone(),
        source,
    })
}

pub fn save(path: &PathBuf, settings: &AppSettings) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SettingsError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(SettingsError::Serialize)?;
    fs::write(path, text).map_err(|source| SettingsError::Write {
        path: path.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_and_reloading_settings_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let mut settings = AppSettings::default();
        settings.sampling_interval_ms = 33;
        settings.export_directory = PathBuf::from("C:/exports");

        save(&path, &settings).expect("save settings");
        let loaded = load_or_default(&path).expect("load settings");

        assert_eq!(loaded, settings);
    }

    #[test]
    fn parse_failure_never_overwrites_user_settings() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        fs::write(&path, "{ user data that needs repair").unwrap();
        assert!(matches!(
            load_or_default(&path),
            Err(SettingsError::Parse { .. })
        ));
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "{ user data that needs repair"
        );
    }
}

#[derive(Debug, Clone)]
pub struct DebouncedSettingsSave {
    pub delay: std::time::Duration,
    pub pending_deadline: Option<std::time::Instant>,
}
impl DebouncedSettingsSave {
    pub fn new(delay: std::time::Duration) -> Self {
        Self {
            delay,
            pending_deadline: None,
        }
    }
    pub fn schedule_at(&mut self, now: std::time::Instant) {
        self.pending_deadline = Some(now + self.delay)
    }
    pub fn should_save(&self, now: std::time::Instant) -> bool {
        self.pending_deadline.is_some_and(|d| now >= d)
    }
    pub fn mark_saved(&mut self) {
        self.pending_deadline = None
    }
}

#[cfg(test)]
mod debounce_tests {
    use super::*;
    #[test]
    fn debounced_settings_save_scheduling() {
        let now = std::time::Instant::now();
        let mut d = DebouncedSettingsSave::new(std::time::Duration::from_millis(350));
        d.schedule_at(now);
        assert!(!d.should_save(now + std::time::Duration::from_millis(349)));
        assert!(d.should_save(now + std::time::Duration::from_millis(350)));
        d.mark_saved();
        assert!(!d.should_save(now + std::time::Duration::from_secs(1)));
    }
}
