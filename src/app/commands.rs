use super::state::AppState;
use crate::{
    export::image_export::{export_png, ExportBackground, ExportOptions},
    session::{
        model::RecordingStatus,
        recovery::{self, RecoveryState},
    },
};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSessionOutcome {
    ClearPreviousCanvas,
    PreserveForExport,
    Cancel,
}

impl AppState {
    pub fn request_start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Stopped
            && self.has_unexported_canvas
            && !self.canvas.is_empty()
        {
            self.pending_new_session_decision = true;
            self.status_message = Some(
                "Previous canvas has not been exported: clear, export/preserve, or cancel."
                    .to_owned(),
            );
        } else {
            self.start_recording();
        }
    }

    pub fn resolve_new_session(&mut self, outcome: NewSessionOutcome) {
        self.pending_new_session_decision = false;
        match outcome {
            NewSessionOutcome::ClearPreviousCanvas => {
                self.clear_canvas_internal();
                self.start_recording();
            }
            NewSessionOutcome::PreserveForExport => {
                self.status_message = Some("Previous canvas preserved. Export or clear it before starting another session.".to_owned());
            }
            NewSessionOutcome::Cancel => {
                self.status_message =
                    Some("New session canceled; previous canvas preserved.".to_owned());
            }
        }
    }

    pub fn start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Stopped {
            if self.has_unexported_canvas && !self.canvas.is_empty() {
                self.request_start_recording();
                return;
            }
            self.recording_status = RecordingStatus::Recording;
            self.mark_started_now();
            self.movement_classifier =
                crate::session::controller::MovementClassifier::new(&self.settings);
            self.start_sampler();
            self.status_message = Some("Recording started.".to_owned());
        }
    }

    pub fn pause_recording(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.recording_status = RecordingStatus::Paused;
            self.stop_sampler();
            self.movement_classifier
                .mark_discontinuity(crate::session::controller::DiscontinuityReason::PauseResume);
            self.autosave_recovery(false);
            self.status_message = Some("Recording paused.".to_owned());
        }
    }

    pub fn resume_recording(&mut self) {
        if self.recording_status == RecordingStatus::Paused {
            self.recording_status = RecordingStatus::Recording;
            self.movement_classifier
                .mark_discontinuity(crate::session::controller::DiscontinuityReason::PauseResume);
            self.start_sampler();
            self.status_message = Some("Recording resumed.".to_owned());
        }
    }

    pub fn toggle_pause_resume(&mut self) {
        match self.recording_status {
            RecordingStatus::Recording => self.pause_recording(),
            RecordingStatus::Paused => self.resume_recording(),
            RecordingStatus::Stopped => {}
        }
    }

    pub fn finish_session(&mut self) {
        if matches!(
            self.recording_status,
            RecordingStatus::Recording | RecordingStatus::Paused
        ) {
            self.stop_sampler();
            self.movement_classifier.finalize_active_segment();
            self.movement_classifier.finalize_dwell();
            self.recording_status = RecordingStatus::Stopped;
            self.sync_retained_canvas_and_statistics();
            self.timing.started_at = None;
            self.has_unexported_canvas = !self.canvas.is_empty();
            self.autosave_recovery(true);
            self.status_message = Some(format!(
                "Session finished. Samples: {}, movements: {}, dwells: {}. Export is available.",
                self.statistics.samples_recorded,
                self.statistics.movement_segment_count,
                self.statistics.finalized_dwell_count
            ));
        }
    }

    pub fn export_canvas_to_default(&mut self) {
        let options = ExportOptions {
            destination: None,
            default_directory: self.settings.export_directory.clone(),
            session_name: Some("session".to_owned()),
            timestamp: SystemTime::now(),
            custom_size: None,
            background: if self.settings.transparent_canvas_mode {
                ExportBackground::Transparent
            } else {
                ExportBackground::Solid
            },
        };
        match export_png(&self.canvas, &options) {
            Ok(path) => {
                self.has_unexported_canvas = false;
                self.status_message = Some(format!("Exported PNG to {}", path.display()));
            }
            Err(e) => self.status_message = Some(format!("Export failed: {e}")),
        }
    }

    pub fn clear_canvas_when_safe(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.status_message =
                Some("Pause or finish recording before clearing the canvas.".to_owned());
            return;
        }
        if self.has_unexported_canvas && !self.canvas.is_empty() {
            self.status_message = Some("Canvas has unexported data. Choose clear previous canvas, preserve/export, or cancel new session.".to_owned());
            return;
        }
        self.clear_canvas_internal();
        self.status_message = Some("Canvas and statistics cleared.".to_owned());
    }

    fn clear_canvas_internal(&mut self) {
        self.canvas.clear();
        self.statistics.reset();
        self.has_unexported_canvas = false;
        self.movement_classifier =
            crate::session::controller::MovementClassifier::new(&self.settings);
    }

    pub fn autosave_recovery(&mut self, completed: bool) {
        self.sync_retained_canvas_and_statistics();
        if let Some(path) = &self.recovery_path {
            let state = RecoveryState {
                canvas: self.canvas.clone(),
                session_name: None,
                saved_at: SystemTime::now(),
                application_colors: self.settings.application_colors.clone(),
                statistics: self.statistics.clone(),
                virtual_desktop_bounds: self.canvas.virtual_desktop_bounds,
                completed,
            };
            let _ = recovery::save_recovery(path, &state);
        }
    }

    pub fn restore_recovery(&mut self) {
        if let Some(path) = &self.recovery_path {
            if let Ok(r) = recovery::load_recovery(path) {
                self.canvas = r.canvas;
                self.statistics = r.statistics;
                self.settings.application_colors = r.application_colors;
                self.has_unexported_canvas = true;
                self.status_message = Some("Recovered incomplete session canvas.".to_owned());
            }
        }
    }
    pub fn discard_recovery(&mut self) {
        if let Some(path) = &self.recovery_path {
            let _ = recovery::discard_recovery(path);
            self.status_message = Some("Recovery data discarded.".to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{coordinates::CanvasPoint, model::MovementPath},
        capture::sampler::CursorSample,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn can_install_fake_sampler_for_state_tests() {
        let mut state = AppState::default();
        let sampler = crate::capture::sampler::FakeCursorSampler::new(Vec::new());
        state.install_sampler_for_tests(Box::new(sampler));
        state.start_sampler();
        state.stop_sampler();
    }
    struct FailingResolver;
    impl crate::capture::foreground::ForegroundResolver for FailingResolver {
        fn resolve_foreground(
            &mut self,
        ) -> Result<
            crate::capture::foreground::ForegroundApplication,
            crate::capture::foreground::ForegroundError,
        > {
            Err(crate::capture::foreground::ForegroundError(
                "boom".to_owned(),
            ))
        }
    }
    #[test]
    fn unknown_foreground_resolution_falls_back_without_error() {
        let now = Instant::now();
        let mut state = AppState::default();
        state.install_foreground_resolver_for_tests(Box::new(FailingResolver));
        state.install_sampler_for_tests(Box::new(crate::capture::sampler::FakeCursorSampler::new(
            vec![CursorSample::new(now, 1.0, 2.0)],
        )));
        state.start_sampler();
        state.drain_samples();
        state.stop_sampler();
        assert_eq!(state.statistics.samples_recorded, 1);
        assert_eq!(
            state
                .current_foreground_application
                .identity
                .executable_name,
            "unknown/system"
        );
    }
    #[test]
    fn recording_status_command_transitions() {
        let mut state = AppState::default();
        state.start_recording();
        assert_eq!(state.recording_status, RecordingStatus::Recording);
        assert!(state.timing.started_at.is_some());
        state.pause_recording();
        assert_eq!(state.recording_status, RecordingStatus::Paused);
        state.resume_recording();
        assert_eq!(state.recording_status, RecordingStatus::Recording);
        state.finish_session();
        assert_eq!(state.recording_status, RecordingStatus::Stopped);
        assert!(state.timing.started_at.is_none());
    }
    #[test]
    fn clear_command_resets_canvas_and_statistics_placeholders() {
        let mut state = AppState::default();
        let mut path = MovementPath::new(state.settings.default_movement_color.clone(), 2.0, true);
        path.points.push(CanvasPoint { x: 1.0, y: 2.0 });
        state.canvas.finalized_movement_paths.push(path);
        state.statistics.samples_recorded = 10;
        state.clear_canvas_when_safe();
        assert!(state.canvas.is_empty());
        assert_eq!(state.statistics.samples_recorded, 0);
    }
    #[test]
    fn finish_finalizes_active_movement_and_dwell() {
        let t0 = Instant::now();
        let mut state = AppState::default();
        state.settings.movement_threshold_px = 5.0;
        state.settings.dwell_activation_delay_ms = 50;
        state.start_recording();
        state
            .movement_classifier
            .accept_sample(CursorSample::new(t0, 0.0, 0.0));
        state.movement_classifier.accept_sample(CursorSample::new(
            t0 + Duration::from_millis(10),
            10.0,
            0.0,
        ));
        state.movement_classifier.accept_sample(CursorSample::new(
            t0 + Duration::from_millis(20),
            20.0,
            0.0,
        ));
        state.movement_classifier.accept_sample(CursorSample::new(
            t0 + Duration::from_millis(100),
            20.0,
            0.0,
        ));
        state.movement_classifier.accept_sample(CursorSample::new(
            t0 + Duration::from_millis(160),
            20.0,
            0.0,
        ));
        state.finish_session();
        assert_eq!(state.canvas.finalized_movement_paths.len(), 1);
        assert_eq!(state.canvas.finalized_dwell_shapes.len(), 1);
        assert!(state.canvas.active_movement_segment.is_none());
        assert!(state.canvas.active_dwell_shape.is_none());
        assert!(state.has_unexported_canvas);
    }
    #[test]
    fn starting_new_session_does_not_silently_discard_unexported_work() {
        let mut state = AppState::default();
        let mut path = MovementPath::new(state.settings.default_movement_color.clone(), 2.0, true);
        path.points.push(CanvasPoint { x: 1.0, y: 2.0 });
        path.points.push(CanvasPoint { x: 2.0, y: 3.0 });
        state.canvas.finalized_movement_paths.push(path);
        state.has_unexported_canvas = true;
        state.start_recording();
        assert_eq!(state.recording_status, RecordingStatus::Stopped);
        assert!(state.pending_new_session_decision);
        assert!(!state.canvas.is_empty());
        state.resolve_new_session(NewSessionOutcome::Cancel);
        assert_eq!(state.recording_status, RecordingStatus::Stopped);
        assert!(!state.canvas.is_empty());
    }
}
