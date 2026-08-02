use crate::session::{
    model::RecordingStatus,
    shutdown::{ShutdownProgress, ShutdownResult, ShutdownTicket},
};
use std::time::{Duration, Instant};

pub const FORCE_BUTTON_DELAY: Duration = Duration::from_secs(2);
pub const HARD_DEADLINE: Duration = Duration::from_secs(5);

/// Process termination seam. Tests use a recorder; production is the only
/// implementation that calls the non-destructor-dependent exit path.
pub trait ProcessTerminator {
    fn terminate(&self, code: i32);
}
pub struct SystemProcessTerminator;
impl ProcessTerminator for SystemProcessTerminator {
    fn terminate(&self, code: i32) {
        std::process::exit(code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitSource {
    WindowClose,
    Tray,
    CliOrIpc,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    Running,
    ExitConfirmationRequested,
    StartingShutdown,
    WaitingForEngineAndRecovery,
    RecoveryFailedAwaitingChoice,
    ReadyToClose,
    ForceExiting,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitRequestResult {
    ConfirmationRequired,
    BeginShutdown,
    AlreadyInProgress,
}

pub struct LifecycleCoordinator {
    state: LifecycleState,
    source: Option<ExitSource>,
    started: Option<Instant>,
    ticket: Option<ShutdownTicket>,
    pub progress: Option<ShutdownProgress>,
    pub outcome: Option<ShutdownResult>,
    pub final_close_sent: bool,
}
impl Default for LifecycleCoordinator {
    fn default() -> Self {
        Self {
            state: LifecycleState::Running,
            source: None,
            started: None,
            ticket: None,
            progress: None,
            outcome: None,
            final_close_sent: false,
        }
    }
}
impl LifecycleCoordinator {
    pub fn state(&self) -> &LifecycleState {
        &self.state
    }
    pub fn source(&self) -> Option<ExitSource> {
        self.source
    }
    pub fn is_preparing(&self) -> bool {
        !matches!(
            self.state,
            LifecycleState::Running | LifecycleState::ExitConfirmationRequested
        )
    }
    pub fn confirmation_pending(&self) -> bool {
        self.state == LifecycleState::ExitConfirmationRequested
    }
    pub fn exit_requested(
        &mut self,
        source: ExitSource,
        needs: bool,
        now: Instant,
    ) -> ExitRequestResult {
        if self.state != LifecycleState::Running {
            return ExitRequestResult::AlreadyInProgress;
        }
        self.source = Some(source);
        if needs {
            self.state = LifecycleState::ExitConfirmationRequested;
            ExitRequestResult::ConfirmationRequired
        } else {
            self.started = Some(now);
            self.state = LifecycleState::ReadyToClose;
            ExitRequestResult::BeginShutdown
        }
    }
    pub fn cancel_confirmation(&mut self) {
        if self.confirmation_pending() {
            *self = Self::default()
        }
    }
    pub fn confirm_exit(&mut self, now: Instant) -> bool {
        if !self.confirmation_pending() {
            return false;
        }
        self.started = Some(now);
        self.state = LifecycleState::StartingShutdown;
        true
    }
    pub fn start(&mut self, ticket: ShutdownTicket) {
        if self.state == LifecycleState::StartingShutdown {
            self.ticket = Some(ticket);
            self.state = LifecycleState::WaitingForEngineAndRecovery
        }
    }
    pub fn poll(&mut self) {
        let Some(t) = &self.ticket else { return };
        while let Some(p) = t.try_progress() {
            self.progress = Some(p)
        }
        match t.try_result() {
            Ok(Some(r)) => {
                let failed = matches!(
                    r,
                    ShutdownResult::RecoveryFailedWorkersStopped { .. }
                        | ShutdownResult::WorkerShutdownFailed { .. }
                );
                self.outcome = Some(r);
                self.state = if failed {
                    LifecycleState::RecoveryFailedAwaitingChoice
                } else {
                    LifecycleState::ReadyToClose
                }
            }
            Err(_) => {
                self.outcome = Some(ShutdownResult::WorkerShutdownFailed {
                    error: "shutdown result channel disconnected".into(),
                    previous_recovery_valid: true,
                });
                self.state = LifecycleState::RecoveryFailedAwaitingChoice
            }
            _ => {}
        }
    }
    pub fn elapsed(&self, now: Instant) -> Duration {
        self.started
            .map(|s| now.saturating_duration_since(s))
            .unwrap_or_default()
    }
    pub fn force_visible(&self, now: Instant) -> bool {
        self.elapsed(now) >= FORCE_BUTTON_DELAY
    }
    pub fn deadline_reached(&self, now: Instant) -> bool {
        self.is_preparing() && self.elapsed(now) >= HARD_DEADLINE
    }
    pub fn force(&mut self) {
        self.state = LifecycleState::ForceExiting
    }
    pub fn exit_anyway(&mut self) -> bool {
        if self
            .outcome
            .as_ref()
            .is_some_and(ShutdownResult::workers_stopped)
        {
            self.state = LifecycleState::ReadyToClose;
            true
        } else {
            false
        }
    }
    pub fn cancel_after_failure(&mut self) -> bool {
        false
    } // a terminated authoritative engine cannot safely resume
}
pub fn confirmation_worthy(
    status: RecordingStatus,
    canvas_nonempty: bool,
    recoverable_or_unexported: bool,
) -> bool {
    status != RecordingStatus::Stopped || canvas_nonempty || recoverable_or_unexported
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thresholds_are_exact() {
        let n = Instant::now();
        let mut c = LifecycleCoordinator::default();
        c.exit_requested(ExitSource::Tray, true, n);
        c.confirm_exit(n);
        assert!(!c.force_visible(n + FORCE_BUTTON_DELAY - Duration::from_nanos(1)));
        assert!(c.force_visible(n + FORCE_BUTTON_DELAY));
        assert!(c.deadline_reached(n + HARD_DEADLINE));
    }
    #[test]
    fn empty_is_ready_without_ticket() {
        let n = Instant::now();
        let mut c = LifecycleCoordinator::default();
        c.exit_requested(ExitSource::WindowClose, false, n);
        assert_eq!(c.state(), &LifecycleState::ReadyToClose);
    }
}
