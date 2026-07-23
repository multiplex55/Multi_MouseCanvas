use crate::{
    canvas::model::CanvasModel,
    capture::{
        foreground::ForegroundApplicationPlaceholder,
        sampler::{CursorSample, CursorSampler},
        windows::WindowsPollingSampler,
    },
    session::{
        controller::MovementClassifier,
        model::{RecordingStatus, SessionTiming},
        statistics::SessionStatistics,
    },
    settings::{model::AppSettings, storage},
};
use std::{path::PathBuf, sync::mpsc::Receiver, time::SystemTime};

pub struct AppState {
    pub recording_status: RecordingStatus,
    pub timing: SessionTiming,
    pub canvas: CanvasModel,
    pub current_cursor_sample: Option<CursorSample>,
    pub movement_classifier: MovementClassifier,
    sampler: Option<Box<dyn CursorSampler>>,
    sample_rx: Option<Receiver<CursorSample>>,
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
            movement_classifier: MovementClassifier::new(&AppSettings::default()),
            sampler: None,
            sample_rx: None,
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
        state.movement_classifier = MovementClassifier::new(&state.settings);
        if state.settings.start_recording_automatically {
            state.start_recording();
        }
        state
    }

    pub fn mark_started_now(&mut self) {
        self.timing.started_at = Some(SystemTime::now());
    }

    #[cfg(test)]
    pub fn install_sampler_for_tests(&mut self, sampler: Box<dyn CursorSampler>) {
        self.sampler = Some(sampler);
    }

    pub fn start_sampler(&mut self) {
        if self.sampler.is_none() {
            self.sampler = Some(Box::new(WindowsPollingSampler::new(
                self.settings.sampling_interval_ms,
            )));
        }
        if let Some(sampler) = &mut self.sampler {
            self.sample_rx = Some(sampler.start());
        }
    }

    pub fn stop_sampler(&mut self) {
        if let Some(sampler) = &mut self.sampler {
            sampler.stop();
        }
        self.sample_rx = None;
    }

    pub fn drain_samples(&mut self) {
        let mut drained = Vec::new();
        if let Some(rx) = &self.sample_rx {
            while let Ok(sample) = rx.try_recv() {
                drained.push(sample);
            }
        }
        for sample in drained {
            self.statistics.samples_recorded += 1;
            self.current_cursor_sample = Some(sample.clone());
            self.movement_classifier.accept_sample(sample);
            self.statistics.movements_recorded = self.movement_classifier.segments.len() as u64
                + u64::from(self.movement_classifier.total_distance > 0.0);
            self.statistics.dwell_events = self.movement_classifier.dwells.len() as u64;
        }
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

impl Drop for AppState {
    fn drop(&mut self) {
        self.stop_sampler();
    }
}
