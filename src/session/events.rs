use crate::display_profiles::DisplayProfileSnapshot;
use crate::{
    app_colors::registry::ApplicationColorRegistry, canvas::topology::DisplayTopology,
    settings::model::AppSettings,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDisplayProfile {
    pub settings: Arc<AppSettings>,
    pub detected_topology: DisplayTopology,
    pub effective_topology: DisplayTopology,
    pub profile: Arc<DisplayProfileSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    Start(ResolvedDisplayProfile),
    Pause,
    Resume,
    Finish,
    Clear,
    RestoreStoppedSession(PathBuf),
    UpdateRecordingParameters(AppSettings),
    UpdateDrawingStyle(AppSettings),
    UpdateApplicationColorRules(ApplicationColorRegistry),
    RefreshTopology(Option<DisplayTopology>),
    InvalidateTopology,
    UpdateBackground(AppSettings),
    SetUiVisibility(bool),
    RequestExport(PathBuf),
    RequestRecoveryCheckpoint,
    RequestSnapshot,
    PrepareShutdown,
    ForceShutdown,
}

impl EngineCommand {
    pub fn priority(&self) -> CommandPriority {
        match self {
            Self::Pause
            | Self::Finish
            | Self::Clear
            | Self::PrepareShutdown
            | Self::ForceShutdown
            | Self::RequestRecoveryCheckpoint
            | Self::InvalidateTopology
            | Self::RefreshTopology(_) => CommandPriority::High,
            Self::Start(_)
            | Self::Resume
            | Self::RestoreStoppedSession(_)
            | Self::UpdateRecordingParameters(_)
            | Self::UpdateDrawingStyle(_)
            | Self::UpdateApplicationColorRules(_)
            | Self::UpdateBackground(_)
            | Self::SetUiVisibility(_)
            | Self::RequestExport(_)
            | Self::RequestSnapshot => CommandPriority::Normal,
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
