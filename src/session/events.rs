use crate::{
    app_colors::registry::ApplicationColorRegistry, canvas::topology::DisplayTopology,
    settings::model::AppSettings,
};

#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    Start,
    Pause,
    Resume,
    Finish,
    Clear,
    UpdateDrawingSettings(AppSettings),
    UpdateApplicationColorRules(ApplicationColorRegistry),
    RefreshTopology(Option<DisplayTopology>),
    RequestSnapshot,
    Shutdown,
}

impl EngineCommand {
    pub fn priority(&self) -> CommandPriority {
        match self {
            Self::Pause
            | Self::Finish
            | Self::Clear
            | Self::Shutdown
            | Self::RefreshTopology(_) => CommandPriority::High,
            Self::Start
            | Self::Resume
            | Self::UpdateDrawingSettings(_)
            | Self::UpdateApplicationColorRules(_)
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
