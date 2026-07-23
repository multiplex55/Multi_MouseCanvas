//! Foreground application identity model.
//!
//! Privacy note: the default foreground model intentionally records only process
//! metadata (PID, executable name/path, optional window class, optional display
//! label). It does not collect window titles, typed text, window contents,
//! browser URLs, clicks, or keyboard data.

use serde::{Deserialize, Serialize};
use std::{fmt, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApplicationIdentity {
    pub process_id: u32,
    pub executable_name: String,
    pub executable_path: Option<String>,
    pub window_class: Option<String>,
}

impl ApplicationIdentity {
    pub fn new(
        process_id: u32,
        executable_name: impl Into<String>,
        executable_path: Option<String>,
        window_class: Option<String>,
    ) -> Self {
        Self {
            process_id,
            executable_name: normalize_executable_name(&executable_name.into()),
            executable_path: executable_path.map(|p| normalize_executable_path(&p)),
            window_class: window_class.and_then(normalize_optional_label),
        }
    }

    pub fn unknown() -> Self {
        Self::new(0, "unknown/system", None, Some("system".to_owned()))
    }

    pub fn stable_key(&self) -> String {
        self.executable_path
            .clone()
            .unwrap_or_else(|| self.executable_name.clone())
    }
}

impl Default for ApplicationIdentity {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForegroundApplication {
    pub identity: ApplicationIdentity,
    pub display_label: Option<String>,
}

impl ForegroundApplication {
    pub fn unknown() -> Self {
        Self {
            identity: ApplicationIdentity::unknown(),
            display_label: Some("Unknown/System".to_owned()),
        }
    }

    pub fn label(&self) -> &str {
        self.display_label
            .as_deref()
            .unwrap_or(&self.identity.executable_name)
    }
}

impl Default for ForegroundApplication {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundError(pub String);

impl fmt::Display for ForegroundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ForegroundError {}

pub trait ForegroundResolver: Send {
    fn resolve_foreground(&mut self) -> Result<ForegroundApplication, ForegroundError>;
}

pub fn normalize_executable_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed == "unknown/system" {
        return trimmed.to_owned();
    }
    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(trimmed);
    normalize_case(file_name)
}

pub fn normalize_executable_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    normalize_case(&normalized)
}

fn normalize_optional_label(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(windows)]
fn normalize_case(value: &str) -> String {
    value.to_ascii_lowercase()
}

#[cfg(not(windows))]
fn normalize_case(value: &str) -> String {
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_path_name_normalization_prevents_duplicate_casing() {
        let a = ApplicationIdentity::new(
            1,
            "APP.EXE",
            Some("C:\\Program Files\\App\\APP.EXE".into()),
            None,
        );
        let b = ApplicationIdentity::new(
            2,
            "app.exe",
            Some("c:/program files/app/app.exe".into()),
            None,
        );
        assert_eq!(
            a.stable_key().to_ascii_lowercase(),
            b.stable_key().to_ascii_lowercase()
        );
        assert_eq!(
            normalize_executable_name("C:/X/APP.EXE").to_ascii_lowercase(),
            "app.exe"
        );
    }
}
