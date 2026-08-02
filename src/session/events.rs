use super::shutdown::{ShutdownRequest, ShutdownResult};
use crate::display_profiles::DisplayProfileSnapshot;
use crate::{
    app_colors::registry::ApplicationColorRegistry, canvas::topology::DisplayTopology,
    settings::model::AppSettings,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::{path::PathBuf, sync::Arc, time::SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExportRequestId(u64);
impl ExportRequestId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportRequest {
    pub id: ExportRequestId,
    pub destination: crate::export::model::ExportDestination,
    pub format: crate::export::model::ExportFormat,
    pub timestamp: SystemTime,
    pub scale: crate::export::model::ExportScale,
    pub background: crate::export::model::ExportBackground,
    pub panels: crate::export::model::InformationPanels,
}
impl ExportRequest {
    pub fn png(directory: PathBuf) -> Self {
        Self {
            id: ExportRequestId::next(),
            destination: crate::export::model::ExportDestination::Directory(directory),
            format: crate::export::model::ExportFormat::Png,
            timestamp: SystemTime::now(),
            scale: crate::export::model::ExportScale::Full,
            background: crate::export::model::ExportBackground::Transparent,
            panels: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportRejection {
    ConcurrentExport,
    NotExportable,
    UnsupportedFormat,
    ShuttingDown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportResult {
    Success {
        request_id: ExportRequestId,
        path: PathBuf,
    },
    Failure {
        request_id: ExportRequestId,
        error: String,
        retry_request: ExportRequest,
    },
    Rejected {
        request_id: ExportRequestId,
        reason: ExportRejection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionRequestId(u64);
impl TransitionRequestId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    Start,
    Pause,
    Resume,
    Finish,
    Clear,
}
#[derive(Debug, Clone)]
pub struct TransitionRequest<T = ()> {
    pub id: TransitionRequestId,
    pub data: T,
}
impl<T> TransitionRequest<T> {
    pub fn new(data: T) -> Self {
        Self {
            id: TransitionRequestId::next(),
            data,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionRejection {
    WrongCurrentState {
        current: crate::session::model::RecordingStatus,
    },
    ShutdownInProgress,
    EmptyEffectiveTopology,
    OperationInProgress,
}
impl TransitionRejection {
    pub fn message(&self) -> &'static str {
        match self {
            Self::WrongCurrentState { .. } => {
                "That operation is not available in the current recording state."
            }
            Self::ShutdownInProgress => "The recording engine is shutting down.",
            Self::EmptyEffectiveTopology => {
                "Select at least one display before starting recording."
            }
            Self::OperationInProgress => "Another recording operation is already being applied.",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionResult {
    Success,
    Rejected(TransitionRejection),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionAcknowledgment {
    pub request_id: TransitionRequestId,
    pub kind: TransitionKind,
    pub status: crate::session::model::RecordingStatus,
    pub result: TransitionResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FullStateRequestId(u64);
impl FullStateRequestId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDisplayProfile {
    pub settings: Arc<AppSettings>,
    pub detected_topology: DisplayTopology,
    pub effective_topology: DisplayTopology,
    pub profile: Arc<DisplayProfileSnapshot>,
}

#[derive(Debug, Clone)]
pub enum EngineCommand {
    Start(TransitionRequest<ResolvedDisplayProfile>),
    Pause(TransitionRequest),
    Resume(TransitionRequest),
    Finish(TransitionRequest),
    Clear(TransitionRequest),
    RestoreStoppedSession(PathBuf),
    UpdateRecordingParameters(AppSettings),
    UpdateDrawingStyle(AppSettings),
    UpdateApplicationColorRules(ApplicationColorRegistry),
    RefreshTopology(Option<DisplayTopology>),
    InvalidateTopology,
    UpdateBackground(AppSettings),
    SetUiVisibility(bool),
    RequestExport(ExportRequest),
    RequestRecoveryCheckpoint,
    RequestSnapshot,
    RequireFullState(FullStateRequestId),
    PrepareShutdown(ShutdownRequest, Sender<ShutdownResult>),
    ForceShutdown,
}

impl EngineCommand {
    pub fn priority(&self) -> CommandPriority {
        match self {
            Self::Pause(_)
            | Self::Finish(_)
            | Self::Clear(_)
            | Self::PrepareShutdown(..)
            | Self::ForceShutdown
            | Self::RequestRecoveryCheckpoint
            | Self::InvalidateTopology
            | Self::RefreshTopology(_) => CommandPriority::High,
            Self::Start(_)
            | Self::Resume(_)
            | Self::RestoreStoppedSession(_)
            | Self::UpdateRecordingParameters(_)
            | Self::UpdateDrawingStyle(_)
            | Self::UpdateApplicationColorRules(_)
            | Self::UpdateBackground(_)
            | Self::SetUiVisibility(_)
            | Self::RequestExport(_)
            | Self::RequestSnapshot
            | Self::RequireFullState(_) => CommandPriority::Normal,
        }
    }
    pub fn is_high_priority(&self) -> bool {
        self.priority() == CommandPriority::High
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPriority {
    High,
    Normal,
}
