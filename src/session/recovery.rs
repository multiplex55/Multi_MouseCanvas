use crate::{
    app_colors::registry::ApplicationColorRegistry,
    canvas::{coordinates::VirtualDesktopBounds, model::CanvasModel},
    session::statistics::SessionStatistics,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub canvas: CanvasModel,
    pub session_name: Option<String>,
    pub saved_at: SystemTime,
    pub application_colors: ApplicationColorRegistry,
    pub statistics: SessionStatistics,
    pub virtual_desktop_bounds: VirtualDesktopBounds,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    None,
    Incomplete(PathBuf),
}

pub fn autosave_path(base: &Path) -> PathBuf {
    base.join("autosave.recovery.json")
}

pub fn save_recovery(path: &Path, state: &RecoveryState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    fs::write(path, bytes)
}

pub fn load_recovery(path: &Path) -> io::Result<RecoveryState> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub fn detect_incomplete(path: &Path) -> RecoveryStatus {
    match load_recovery(path) {
        Ok(state) if !state.completed => RecoveryStatus::Incomplete(path.to_path_buf()),
        _ => RecoveryStatus::None,
    }
}

pub fn discard_recovery(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn state(completed: bool) -> RecoveryState {
        RecoveryState {
            canvas: CanvasModel::default(),
            session_name: Some("s".into()),
            saved_at: SystemTime::UNIX_EPOCH,
            application_colors: ApplicationColorRegistry::default(),
            statistics: SessionStatistics::default(),
            virtual_desktop_bounds: VirtualDesktopBounds::new(0.0, 0.0, 1.0, 1.0),
            completed,
        }
    }
    #[test]
    fn recovery_detection_identifies_incomplete_sessions() {
        let d = tempdir().unwrap();
        let p = autosave_path(d.path());
        save_recovery(&p, &state(false)).unwrap();
        assert_eq!(detect_incomplete(&p), RecoveryStatus::Incomplete(p));
    }
    #[test]
    fn recovery_discard_removes_or_ignores_recovery_data() {
        let d = tempdir().unwrap();
        let p = autosave_path(d.path());
        save_recovery(&p, &state(false)).unwrap();
        discard_recovery(&p).unwrap();
        assert_eq!(detect_incomplete(&p), RecoveryStatus::None);
        discard_recovery(&p).unwrap();
    }
}
