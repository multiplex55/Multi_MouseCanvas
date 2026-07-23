use super::state::AppState;
use crate::session::model::RecordingStatus;

impl AppState {
    pub fn start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Stopped {
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
            self.recording_status = RecordingStatus::Stopped;
            self.stop_sampler();
            self.movement_classifier.finalize_dwell();
            self.movement_classifier.finalize_active_segment();
            self.sync_retained_canvas_and_statistics();
            self.timing.started_at = None;
            self.status_message = Some("Session finished.".to_owned());
        }
    }

    pub fn clear_canvas_when_safe(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.status_message =
                Some("Pause or finish recording before clearing the canvas.".to_owned());
            return;
        }
        self.canvas.clear();
        self.statistics.reset();
        self.movement_classifier =
            crate::session::controller::MovementClassifier::new(&self.settings);
        self.status_message = Some("Canvas and statistics cleared.".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{coordinates::CanvasPoint, model::MovementPath};

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
        let now = std::time::Instant::now();
        let mut state = AppState::default();
        state.install_foreground_resolver_for_tests(Box::new(FailingResolver));
        state.install_sampler_for_tests(Box::new(crate::capture::sampler::FakeCursorSampler::new(
            vec![crate::capture::sampler::CursorSample::new(now, 1.0, 2.0)],
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
        state.statistics.movements_recorded = 5;
        state.statistics.dwell_events = 2;

        state.clear_canvas_when_safe();

        assert!(state.canvas.is_empty());
        assert_eq!(state.statistics.samples_recorded, 0);
        assert_eq!(state.statistics.movements_recorded, 0);
        assert_eq!(state.statistics.dwell_events, 0);
    }
}
