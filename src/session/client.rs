use super::{
    engine::{RecordingEngineHandle, SubmitError},
    events::EngineCommand,
    snapshot::SessionSnapshot,
};
use std::sync::mpsc::TryRecvError;

/// The deliberately small UI boundary. It exposes semantic commands and
/// immutable snapshots, never sampler channels or mutable engine state.
pub struct RecordingEngineClient {
    handle: RecordingEngineHandle,
}
impl RecordingEngineClient {
    pub fn new(handle: RecordingEngineHandle) -> Self {
        Self { handle }
    }
    pub fn submit(&self, command: EngineCommand) -> Result<(), SubmitError> {
        self.handle.try_submit(command)
    }
    pub fn try_snapshot(&self) -> Result<Option<SessionSnapshot>, SubmitError> {
        match self.handle.snapshot_rx.try_recv() {
            Ok(s) => Ok(Some(s)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(SubmitError::Disconnected),
        }
    }
    pub fn try_acknowledgment(
        &self,
    ) -> Result<Option<super::events::TransitionAcknowledgment>, SubmitError> {
        match self.handle.acknowledgment_rx.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(SubmitError::Disconnected),
        }
    }
    pub fn begin_orderly_shutdown(
        &mut self,
        request: super::shutdown::ShutdownRequest,
    ) -> super::shutdown::ShutdownTicket {
        self.handle.begin_orderly_shutdown(request)
    }
    pub fn force_shutdown(&mut self) {
        self.handle.force_shutdown()
    }
}
