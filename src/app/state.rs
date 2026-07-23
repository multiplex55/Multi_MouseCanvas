use crate::{
    canvas::{
        coordinates::CanvasPoint,
        model::{CanvasModel, DwellShape, MovementPath},
    },
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
use std::{
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, SystemTime},
};

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
            self.sync_retained_canvas_and_statistics();
        }
    }

    pub fn sync_retained_canvas_and_statistics(&mut self) {
        self.canvas.background.color = self.settings.background_color.clone();
        self.canvas.background.transparent = self.settings.transparent_canvas_mode;

        self.canvas.finalized_movement_paths = self
            .movement_classifier
            .segments
            .iter()
            .map(|segment| self.path_from_points(&segment.points, true))
            .collect();
        self.canvas.active_movement_segment = self
            .movement_classifier
            .active_segment()
            .map(|segment| self.path_from_points(&segment.points, false));

        self.canvas.finalized_dwell_shapes = self
            .movement_classifier
            .dwells
            .iter()
            .map(|dwell| self.dwell_shape(dwell.center_x, dwell.center_y, dwell.duration, true))
            .collect();
        self.canvas.active_dwell_shape = self
            .movement_classifier
            .active_dwell()
            .map(|dwell| self.dwell_shape(dwell.center_x, dwell.center_y, dwell.duration, false));

        self.statistics.total_cursor_distance = self.movement_classifier.total_distance;
        self.statistics.finalized_dwell_count = self.movement_classifier.dwells.len() as u64;
        self.statistics.dwell_events = self.statistics.finalized_dwell_count;
        self.statistics.current_dwell_duration = self.movement_classifier.current_dwell_duration();
        self.statistics.longest_dwell = self
            .movement_classifier
            .dwells
            .iter()
            .map(|d| d.duration)
            .chain(std::iter::once(self.statistics.current_dwell_duration))
            .max()
            .unwrap_or(Duration::ZERO);
        self.statistics.movement_segment_count = self.movement_classifier.segments.len() as u64
            + u64::from(self.movement_classifier.active_segment().is_some());
        self.statistics.movements_recorded = self.statistics.movement_segment_count;
        self.statistics.session_duration = self
            .timing
            .started_at
            .and_then(|started| started.elapsed().ok())
            .unwrap_or(Duration::ZERO);
    }

    fn path_from_points(&self, points: &[(f32, f32)], finalized: bool) -> MovementPath {
        let mut path = MovementPath::new(
            self.settings.default_movement_color.clone(),
            self.settings.line_width_px,
            finalized,
        );
        for (x, y) in points {
            path.push_simplified(
                CanvasPoint { x: *x, y: *y },
                self.canvas.point_merge_distance,
            );
        }
        path
    }

    fn dwell_shape(&self, x: f32, y: f32, duration: Duration, finalized: bool) -> DwellShape {
        DwellShape::from_duration(
            CanvasPoint { x, y },
            duration,
            self.settings.default_dwell_color.clone(),
            self.settings.selected_dwell_shape,
            self.settings.min_dwell_shape_size,
            self.settings.max_dwell_shape_size,
            self.settings.dwell_growth_rate,
            self.settings.dwell_fill_opacity,
            self.settings.dwell_outline_width,
            self.settings.dwell_render_mode,
            finalized,
        )
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
