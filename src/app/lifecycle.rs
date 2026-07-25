use crate::session::model::RecordingStatus;
use std::time::{Duration, Instant};

pub const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitSource {
    WindowClose,
    Tray,
    CliOrIpc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    Running,
    ExitConfirmationRequested { source: ExitSource },
    PreparingShutdown { source: ExitSource },
    WaitingForCheckpoint { source: ExitSource },
    ReadyToClose,
    ShutdownFailed(String),
    ForceExiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitRequestResult {
    ConfirmationRequired,
    BeginShutdown,
    AlreadyInProgress,
}

/// The single authority for all ways in which the application can terminate.
pub struct LifecycleCoordinator {
    state: LifecycleState,
    deadline: Option<Instant>,
    checkpoint_requested: bool,
}

impl Default for LifecycleCoordinator {
    fn default() -> Self {
        Self {
            state: LifecycleState::Running,
            deadline: None,
            checkpoint_requested: false,
        }
    }
}

impl LifecycleCoordinator {
    pub fn state(&self) -> &LifecycleState {
        &self.state
    }
    pub fn is_preparing(&self) -> bool {
        matches!(
            self.state,
            LifecycleState::PreparingShutdown { .. }
                | LifecycleState::WaitingForCheckpoint { .. }
                | LifecycleState::ReadyToClose
                | LifecycleState::ShutdownFailed(_)
                | LifecycleState::ForceExiting
        )
    }
    pub fn confirmation_pending(&self) -> bool {
        matches!(self.state, LifecycleState::ExitConfirmationRequested { .. })
    }
    pub fn exit_requested(
        &mut self,
        source: ExitSource,
        needs_confirmation: bool,
    ) -> ExitRequestResult {
        if !matches!(self.state, LifecycleState::Running) {
            return ExitRequestResult::AlreadyInProgress;
        }
        if needs_confirmation {
            self.state = LifecycleState::ExitConfirmationRequested { source };
            ExitRequestResult::ConfirmationRequired
        } else {
            self.begin(source);
            ExitRequestResult::BeginShutdown
        }
    }
    pub fn cancel_confirmation(&mut self) {
        if self.confirmation_pending() {
            self.state = LifecycleState::Running;
        }
    }
    pub fn confirm_exit(&mut self, now: Instant) -> bool {
        let LifecycleState::ExitConfirmationRequested { source } = self.state else {
            return false;
        };
        self.begin_at(source, now);
        true
    }
    fn begin(&mut self, source: ExitSource) {
        self.begin_at(source, Instant::now());
    }
    fn begin_at(&mut self, source: ExitSource, now: Instant) {
        self.state = LifecycleState::PreparingShutdown { source };
        self.deadline = Some(now + SHUTDOWN_GRACE_PERIOD);
    }
    /// Returns true exactly once, ensuring repeated Exit cannot create checkpoints.
    pub fn take_checkpoint_request(&mut self) -> bool {
        let LifecycleState::PreparingShutdown { source } = self.state else {
            return false;
        };
        if self.checkpoint_requested {
            return false;
        }
        self.checkpoint_requested = true;
        self.state = LifecycleState::WaitingForCheckpoint { source };
        true
    }
    pub fn checkpoint_complete(&mut self, result: Result<(), String>) {
        if !matches!(self.state, LifecycleState::WaitingForCheckpoint { .. }) {
            return;
        }
        self.state = match result {
            Ok(()) => LifecycleState::ReadyToClose,
            Err(e) => LifecycleState::ShutdownFailed(e),
        };
    }
    pub fn poll_timeout(&mut self, now: Instant) -> bool {
        if self.is_preparing()
            && !matches!(
                self.state,
                LifecycleState::ReadyToClose | LifecycleState::ForceExiting
            )
            && self.deadline.is_some_and(|d| now >= d)
        {
            self.state = LifecycleState::ForceExiting;
            return true;
        }
        false
    }
}

pub fn confirmation_worthy(
    status: RecordingStatus,
    canvas_nonempty: bool,
    recoverable_or_unexported: bool,
) -> bool {
    match status {
        RecordingStatus::Recording => true,
        RecordingStatus::Paused => recoverable_or_unexported || canvas_nonempty,
        RecordingStatus::Stopped => canvas_nonempty && recoverable_or_unexported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_policy_covers_recording_and_canvas_states() {
        assert!(confirmation_worthy(
            RecordingStatus::Recording,
            false,
            false
        ));
        assert!(confirmation_worthy(RecordingStatus::Paused, true, true));
        assert!(!confirmation_worthy(RecordingStatus::Stopped, false, false));
        assert!(confirmation_worthy(RecordingStatus::Stopped, true, true));
    }
    #[test]
    fn cancellation_and_repeated_exit_are_idempotent() {
        let mut c = LifecycleCoordinator::default();
        assert_eq!(
            c.exit_requested(ExitSource::WindowClose, true),
            ExitRequestResult::ConfirmationRequired
        );
        c.cancel_confirmation();
        assert!(matches!(c.state(), LifecycleState::Running));
        c.exit_requested(ExitSource::Tray, false);
        assert_eq!(
            c.exit_requested(ExitSource::CliOrIpc, false),
            ExitRequestResult::AlreadyInProgress
        );
        assert!(c.take_checkpoint_request());
        assert!(!c.take_checkpoint_request());
    }
    #[test]
    fn fake_clock_forces_after_deadline() {
        let now = Instant::now();
        let mut c = LifecycleCoordinator::default();
        c.exit_requested(ExitSource::CliOrIpc, true);
        c.confirm_exit(now);
        c.take_checkpoint_request();
        assert!(!c.poll_timeout(now + Duration::from_secs(4)));
        assert!(c.poll_timeout(now + Duration::from_secs(5)));
        assert!(matches!(c.state(), LifecycleState::ForceExiting));
    }
}
