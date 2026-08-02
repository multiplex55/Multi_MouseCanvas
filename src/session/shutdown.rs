use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
};

/// Serializable data needed by the engine to finish a session.  Deliberately
/// contains no `Instant`: UI deadlines belong to the UI process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownRequest {
    pub recovery_directory: Option<PathBuf>,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownProgress {
    FinalizingActivity,
    StoppingSampler,
    SavingRecovery,
    JoiningEngine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownResult {
    RecoverySavedAndWorkersStopped,
    NoRecoveryRequiredAndWorkersStopped,
    RecoveryFailedWorkersStopped {
        error: String,
        previous_recovery_valid: bool,
    },
    WorkerShutdownFailed {
        error: String,
        previous_recovery_valid: bool,
    },
}

impl ShutdownResult {
    pub fn workers_stopped(&self) -> bool {
        !matches!(self, Self::WorkerShutdownFailed { .. })
    }
}

pub struct ShutdownTicket {
    progress: Receiver<ShutdownProgress>,
    result: Receiver<ShutdownResult>,
}

impl ShutdownTicket {
    pub(crate) fn new(
        progress: Receiver<ShutdownProgress>,
        result: Receiver<ShutdownResult>,
    ) -> Self {
        Self { progress, result }
    }
    pub fn try_progress(&self) -> Option<ShutdownProgress> {
        self.progress.try_recv().ok()
    }
    pub fn try_result(&self) -> Result<Option<ShutdownResult>, TryRecvError> {
        match self.result.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
