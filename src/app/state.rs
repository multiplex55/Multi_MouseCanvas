use crate::{
    canvas::model::CanvasModel,
    capture::{
        foreground::ForegroundApplicationPlaceholder,
        sampler::{CursorSamplePlaceholder, DwellStatePlaceholder},
    },
    session::{
        model::{RecordingStatus, SessionTiming},
        statistics::SessionStatistics,
    },
    settings::{model::AppSettings, storage},
};
use std::{path::PathBuf, time::SystemTime};

#[derive(Debug, Clone)]
pub struct AppState {
    pub recording_status: RecordingStatus,
    pub timing: SessionTiming,
    pub canvas: CanvasModel,
    pub current_cursor_sample: Option<CursorSamplePlaceholder>,
    pub current_dwell_state: DwellStatePlaceholder,
    pub current_foreground_application: ForegroundApplicationPlaceholder,
    pub statistics: SessionStatistics,
    pub settings: AppSettings,
    pub status_message: Option<String>,
    pub settings_path: Option<PathBuf>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            recording_status: RecordingStatus::Stopped,
            timing: SessionTiming::default(),
            canvas: CanvasModel::default(),
            current_cursor_sample: None,
            current_dwell_state: DwellStatePlaceholder::default(),
            current_foreground_application: ForegroundApplicationPlaceholder::default(),
            statistics: SessionStatistics::default(),
            settings: AppSettings::default(),
            status_message: None,
            settings_path: None,
        }
    }
}

impl AppState {
    pub fn load() -> Self {
        let mut state = Self::default();
        match storage::default_settings_path() {
            Ok(path) => {
                state.settings_path = Some(path.clone());
                match storage::load_or_default(&path) {
                    Ok(settings) => state.settings = settings,
                    Err(error) => {
                        tracing::warn!(%error, "settings load failed; using defaults");
                        state.status_message =
                            Some(format!("Settings load failed; using defaults: {error}"));
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "settings path unavailable; using defaults");
                state.status_message = Some(format!(
                    "Settings path unavailable; using defaults: {error}"
                ));
            }
        }
        if state.settings.start_recording_automatically {
            state.start_recording();
        }
        state
    }

    pub fn mark_started_now(&mut self) {
        self.timing.started_at = Some(SystemTime::now());
    }

    pub fn save_settings_as_status(&mut self) {
        if let Some(path) = &self.settings_path {
            if let Err(error) = storage::save(path, &self.settings) {
                tracing::warn!(%error, "settings save failed");
                self.status_message = Some(format!("Settings save failed: {error}"));
            }
        }
    }
}
