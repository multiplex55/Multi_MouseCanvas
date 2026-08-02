use crate::{
    app::state::AppState,
    session::{
        client::RecordingEngineClient,
        engine::{RecordingEngineHandle, SubmitError},
        events::EngineCommand,
        snapshot::{EngineConnectionState, SamplerState},
    },
};
use std::time::Instant;

pub struct EngineBridge {
    pub client: RecordingEngineClient,
    disconnected: bool,
}
impl EngineBridge {
    pub fn spawn(settings: crate::settings::model::AppSettings) -> Self {
        Self {
            client: RecordingEngineClient::new(RecordingEngineHandle::spawn(settings, None)),
            disconnected: false,
        }
    }
    pub fn submit(&mut self, state: &mut AppState, c: EngineCommand) -> bool {
        match self.client.submit(c) {
            Ok(()) => true,
            Err(e) => {
                state.status_message = Some(submission_message(e));
                if e == SubmitError::Disconnected {
                    self.mark_disconnected(state)
                }
                false
            }
        }
    }
    pub fn flush_commands(&mut self, state: &mut AppState) {
        for c in std::mem::take(&mut state.engine_commands) {
            self.submit(state, c);
        }
    }
    pub fn drain(&mut self, state: &mut AppState) -> bool {
        let mut more = false;
        for _ in 0..8 {
            match self.client.try_snapshot() {
                Ok(Some(s)) => {
                    state.snapshot_received_at = Some(Instant::now());
                    state.recording_status = s.recording_status;
                    state.statistics = s.statistics.clone();
                    state.capture_health = Some(s.capture_health.clone());
                    state.has_unexported_canvas =
                        !state.preview.is_empty() || !s.tile_deltas.is_empty();
                    if state.preview.apply_snapshot(&s).is_err() {
                        let _ = self.client.submit(EngineCommand::RequestSnapshot);
                    }
                    more = true
                }
                Ok(None) => break,
                Err(_) => {
                    self.mark_disconnected(state);
                    break;
                }
            }
        }
        more
    }
    fn mark_disconnected(&mut self, state: &mut AppState) {
        if self.disconnected {
            return;
        }
        self.disconnected = true;
        state.recording_status = crate::session::model::RecordingStatus::Stopped;
        state.persistent_engine_error =
            Some("Recording engine disconnected. The current preview has been preserved.".into());
        state.capture_health = Some(crate::session::snapshot::CaptureHealth {
            engine: EngineConnectionState::Disconnected,
            sampler: SamplerState::Stopped,
            since_last_observed_sample: None,
            last_engine_sequence: state.preview.latest_sequence,
            engine_error: state.persistent_engine_error.clone(),
        });
    }
    pub fn shutdown(&mut self) {
        self.client.orderly_shutdown();
    }
    pub fn retry(&mut self, state: &mut AppState) {
        self.client.force_shutdown();
        *self = Self::spawn(state.settings.clone());
        state.persistent_engine_error = None;
        self.disconnected = false;
        self.submit(
            state,
            EngineCommand::UpdateRecordingParameters(state.settings.clone()),
        );
        self.submit(
            state,
            EngineCommand::UpdateDrawingStyle(state.settings.clone()),
        );
        self.submit(
            state,
            EngineCommand::UpdateBackground(state.settings.clone()),
        );
        self.submit(
            state,
            EngineCommand::UpdateApplicationColorRules(state.settings.application_colors.clone()),
        );
        self.submit(state, EngineCommand::RequestSnapshot);
        state.status_message = Some("Recording engine restarted in the stopped state.".into());
    }
}
fn submission_message(e: SubmitError) -> String {
    match e {
        SubmitError::QueueFull => "Recording engine is busy; please retry.".into(),
        SubmitError::Disconnected => "Recording engine disconnected; preview preserved.".into(),
        SubmitError::ShuttingDown => "Recording engine is shutting down.".into(),
    }
}
