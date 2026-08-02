use crate::{
    app::state::AppState,
    session::{
        client::RecordingEngineClient,
        engine::{RecordingEngineHandle, SubmitError},
        events::EngineCommand,
        snapshot::{EngineConnectionState, SamplerState},
    },
};
use std::time::{Duration, Instant};

pub struct EngineBridge {
    pub client: RecordingEngineClient,
    disconnected: bool,
    accepted_sequence: u64,
    generation: u64,
}
impl EngineBridge {
    pub fn spawn(settings: crate::settings::model::AppSettings) -> Self {
        Self {
            client: RecordingEngineClient::new(RecordingEngineHandle::spawn(settings, None)),
            disconnected: false,
            accepted_sequence: 0,
            generation: 0,
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
                state.pending_transition = None;
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
        while let Ok(Some(ack)) = self.client.try_acknowledgment() {
            self.reconcile_acknowledgment(state, ack);
            more = true;
        }
        for _ in 0..8 {
            match self.client.try_snapshot() {
                Ok(Some(s)) => {
                    self.reconcile_snapshot(state, s);
                    more = true
                }
                Ok(None) => break,
                Err(_) => {
                    self.mark_disconnected(state);
                    break;
                }
            }
        }
        if state
            .pending_transition
            .as_ref()
            .is_some_and(|p| p.submitted_at.elapsed() >= Duration::from_secs(2))
        {
            state.pending_transition = None;
            state.persistent_engine_error=Some("The recording operation timed out; the current canvas was preserved while state is reconciled.".into());
            let id = crate::session::events::FullStateRequestId::next();
            state.resynchronization = Some(id);
            let _ = self.client.submit(EngineCommand::RequireFullState(id));
        }
        more
    }

    fn accept_generation(&mut self, state: &mut AppState, generation: u64) {
        if generation > self.generation {
            self.generation = generation;
            self.accepted_sequence = 0;
            state.preview.invalidate_generation(generation);
        }
    }

    fn reconcile_acknowledgment(
        &mut self,
        state: &mut AppState,
        ack: crate::session::events::TransitionAcknowledgment,
    ) {
        let correlated = state
            .pending_transition
            .as_ref()
            .is_some_and(|p| p.request_id == ack.request_id && p.kind == ack.kind);
        if !correlated {
            return;
        }
        self.accept_generation(state, ack.generation);
        self.accepted_sequence = self.accepted_sequence.max(ack.sequence);
        state.recording_status = ack.status;
        state.pending_transition = None;
        match ack.result {
            crate::session::events::TransitionResult::Success => {
                if ack.kind == crate::session::events::TransitionKind::Clear
                    && state.start_after_clear
                {
                    state.start_after_clear = false;
                    state.automatic_start_pending = true;
                }
                state.persistent_engine_error = None;
                state.retry_transition = None;
            }
            crate::session::events::TransitionResult::Rejected(reason) => {
                state.persistent_engine_error = Some(reason.message().into())
            }
        }
    }

    fn reconcile_snapshot(
        &mut self,
        state: &mut AppState,
        s: crate::session::snapshot::SessionSnapshot,
    ) {
        let authoritative = state.resynchronization.is_some()
            && state.resynchronization == s.full_state_request_id
            && s.full_tile_snapshot;
        if s.generation < self.generation
            || (!authoritative
                && s.generation == self.generation
                && s.sequence < self.accepted_sequence)
        {
            return;
        }
        self.accept_generation(state, s.generation);
        if authoritative {
            state.preview.prepare_authoritative_full_state(s.generation);
        }
        if state.preview.apply_snapshot(&s).is_err() {
            let _ = self.client.submit(EngineCommand::RequestSnapshot);
            return;
        }
        self.accepted_sequence = self.accepted_sequence.max(s.sequence);
        state.snapshot_received_at = Some(Instant::now());
        state.recording_status = s.recording_status;
        state.statistics = s.statistics.clone();
        state.engine_activity = s.activity.clone();
        if let Some(result) = s.activity.last_export_result.clone() {
            let result_id = match &result {
                crate::session::events::ExportResult::Success { request_id, .. }
                | crate::session::events::ExportResult::Failure { request_id, .. }
                | crate::session::events::ExportResult::Rejected { request_id, .. } => *request_id,
            };
            let is_new = state.export_result.as_ref().map(|old| match old {
                crate::session::events::ExportResult::Success { request_id, .. }
                | crate::session::events::ExportResult::Failure { request_id, .. }
                | crate::session::events::ExportResult::Rejected { request_id, .. } => *request_id,
            }) != Some(result_id);
            let correlated = state.pending_export_requests.remove(&result_id);
            if is_new && correlated {
                match &result {
                    crate::session::events::ExportResult::Success { path, .. } => {
                        state.export_error = None;
                        state.status_message = Some(format!("Export saved to {}", path.display()));
                        if state.export_start_new == Some(result_id) {
                            state.export_start_new = None;
                            state.start_after_clear = true;
                            state.queue_transition(
                                EngineCommand::Clear(
                                    crate::session::events::TransitionRequest::new(()),
                                ),
                                crate::session::events::TransitionKind::Clear,
                                crate::session::model::RecordingStatus::Stopped,
                            );
                        }
                    }
                    crate::session::events::ExportResult::Failure { error, .. } => {
                        state.export_error = Some(error.clone())
                    }
                    crate::session::events::ExportResult::Rejected { reason, .. } => {
                        state.export_error = Some(format!("Export rejected: {reason:?}"))
                    }
                }
                state.export_result = Some(result);
            }
        }
        state.capture_health = Some(s.capture_health.clone());
        state.has_unexported_canvas = !state.preview.is_empty();
        if authoritative {
            state.resynchronization = None;
            state.persistent_engine_error = s.capture_health.engine_error.clone();
        }
    }
    fn mark_disconnected(&mut self, state: &mut AppState) {
        if self.disconnected {
            return;
        }
        self.disconnected = true;
        state.pending_transition = None;
        state.engine_activity.export = crate::session::snapshot::ExportState::Idle;
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
        self.client.force_shutdown();
    }
    pub fn begin_shutdown(
        &mut self,
        request: crate::session::shutdown::ShutdownRequest,
    ) -> crate::session::shutdown::ShutdownTicket {
        self.client.begin_orderly_shutdown(request)
    }
    pub fn force_shutdown(&mut self) {
        self.client.force_shutdown()
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
