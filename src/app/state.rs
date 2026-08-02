use crate::{
    canvas::preview_state::PreviewState,
    display_profiles::ImmutableDisplayProfileSnapshot,
    session::{
        events::{
            EngineCommand, FullStateRequestId, TransitionKind, TransitionRequest,
            TransitionRequestId,
        },
        model::RecordingStatus,
        snapshot::CaptureHealth,
    },
    settings::{model::AppSettings, storage},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct AppState {
    pub recording_status: RecordingStatus,
    pub preview: PreviewState,
    pub statistics: crate::session::statistics::SessionStatistics,
    pub settings: AppSettings,
    pub status_message: Option<String>,
    pub persistent_engine_error: Option<String>,
    pub pending_transition: Option<PendingTransition>,
    pub retry_transition: Option<EngineCommand>,
    pub resynchronization: Option<FullStateRequestId>,
    pub capture_health: Option<CaptureHealth>,
    pub snapshot_received_at: Option<Instant>,
    pub has_unexported_canvas: bool,
    pub pending_new_session_decision: bool,
    pub recovery_path: Option<PathBuf>,
    pub settings_path: Option<PathBuf>,
    pub tray_available: bool,
    pub tray_error: Option<String>,
    pub ui_visible: bool,
    pub minimize_requested: bool,
    pub pending_settings_save: Option<Instant>,
    pub lifecycle_dialogs: crate::app::dialogs::LifecycleDialogState,
    pub performance_diagnostics: crate::app::performance_view::PerformanceDiagnostics,
    /// Read-only mirror of activity confirmed by the engine.
    pub engine_activity: crate::session::snapshot::EngineActivity,
    pub export_result: Option<crate::session::events::ExportResult>,
    pub export_error: Option<String>,
    pub export_start_new: Option<crate::session::events::ExportRequestId>,
    pub pending_export_requests: std::collections::HashSet<crate::session::events::ExportRequestId>,
    pub start_after_clear: bool,
    pub display_profiles: crate::display_profiles::DisplayProfileStore,
    pub display_profiles_path: Option<PathBuf>,
    pub monitor_selection: Option<crate::app::monitor_selection::MonitorSelectionState>,
    pub active_display_profile: Option<ImmutableDisplayProfileSnapshot>,
    pub automatic_start_pending: bool,
    pub retry_engine_requested: bool,
    pub identify_monitors_requested: bool,
    pub monitor_identification_status: Option<String>,
    pub(crate) engine_commands: Vec<EngineCommand>,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            recording_status: RecordingStatus::Stopped,
            preview: Default::default(),
            statistics: Default::default(),
            settings: Default::default(),
            status_message: None,
            persistent_engine_error: None,
            pending_transition: None,
            retry_transition: None,
            resynchronization: None,
            capture_health: None,
            snapshot_received_at: None,
            has_unexported_canvas: false,
            pending_new_session_decision: false,
            recovery_path: None,
            settings_path: None,
            tray_available: false,
            tray_error: None,
            ui_visible: true,
            minimize_requested: false,
            pending_settings_save: None,
            lifecycle_dialogs: Default::default(),
            performance_diagnostics: Default::default(),
            engine_activity: crate::session::snapshot::EngineActivity {
                export: crate::session::snapshot::ExportState::Idle,
                last_export_result: None,
                recovery_in_progress: false,
            },
            export_result: None,
            export_error: None,
            export_start_new: None,
            pending_export_requests: Default::default(),
            start_after_clear: false,
            display_profiles: Default::default(),
            display_profiles_path: None,
            monitor_selection: None,
            active_display_profile: None,
            automatic_start_pending: false,
            retry_engine_requested: false,
            identify_monitors_requested: false,
            monitor_identification_status: None,
            engine_commands: vec![],
        }
    }
}
impl AppState {
    pub fn export_busy(&self) -> bool {
        self.engine_activity.export_in_progress()
    }

    pub fn load() -> Self {
        let mut s = Self::default();
        if let Ok(path) = storage::default_settings_path() {
            s.settings_path = Some(path.clone());
            match storage::load_or_default(&path) {
                Ok(mut v) => {
                    v.validate();
                    s.settings = v
                }
                Err(e) => {
                    s.status_message = Some(format!("Settings load failed; using defaults: {e}"))
                }
            };
            let root = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("recovery");
            s.recovery_path = Some(root);
        }
        if let Ok(path) = crate::display_profiles::default_path() {
            s.display_profiles_path = Some(path.clone());
            if let Ok(v) = crate::display_profiles::DisplayProfileStore::load(&path) {
                s.display_profiles = v
            }
        }
        s.automatic_start_pending = s.settings.start_recording_automatically;
        s
    }
    pub fn canvas(&self) -> &crate::canvas::model::CanvasModel {
        self.preview.canvas()
    }
    pub fn queue(&mut self, c: EngineCommand) {
        if let EngineCommand::RequestExport(request) = &c {
            self.pending_export_requests.insert(request.id);
        }
        self.engine_commands.push(c)
    }
    pub fn queue_transition(
        &mut self,
        command: EngineCommand,
        kind: TransitionKind,
        expected_status: RecordingStatus,
    ) {
        if self.pending_transition.is_some() {
            return;
        }
        let request_id = match &command {
            EngineCommand::Start(r) => r.id,
            EngineCommand::Pause(r) => r.id,
            EngineCommand::Resume(r) => r.id,
            EngineCommand::Finish(r) => r.id,
            EngineCommand::Clear(r) => r.id,
            _ => return,
        };
        self.retry_transition = Some(command.clone());
        self.pending_transition = Some(PendingTransition {
            request_id,
            kind,
            submitted_at: Instant::now(),
            expected_status,
        });
        self.queue(command);
    }
    pub fn schedule_settings_save(&mut self) {
        self.pending_settings_save = Some(Instant::now() + Duration::from_millis(350))
    }
    pub fn flush_settings_save_if_due(&mut self) {
        if self
            .pending_settings_save
            .is_some_and(|d| Instant::now() >= d)
        {
            self.pending_settings_save = None;
            self.save_settings_as_status()
        }
    }
    pub fn save_settings_as_status(&mut self) {
        if let Some(p) = &self.settings_path {
            if let Err(e) = storage::save(p, &self.settings) {
                self.status_message = Some(format!("Settings save failed: {e}"))
            }
        }
    }
    pub fn request_clear_canvas_confirmation(&mut self) {
        self.lifecycle_dialogs
            .request_clear(!self.preview.is_empty());
        if self.preview.is_empty() {
            self.confirm_clear_canvas()
        }
    }
    pub fn confirm_clear_canvas(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.status_message =
                Some("Pause or finish recording before clearing the canvas.".into());
            return;
        }
        self.queue_transition(
            EngineCommand::Clear(TransitionRequest::new(())),
            TransitionKind::Clear,
            RecordingStatus::Stopped,
        );
        self.lifecycle_dialogs.clear_confirmation_open = false
    }
    pub fn apply_settings_update(&mut self, u: SettingsUpdate) {
        let kind = u.kind();
        u.apply(&mut self.settings);
        self.settings.validate();
        let c = match kind {
            SettingKind::Recording => {
                EngineCommand::UpdateRecordingParameters(self.settings.clone())
            }
            SettingKind::Drawing => EngineCommand::UpdateDrawingStyle(self.settings.clone()),
            SettingKind::Background => EngineCommand::UpdateBackground(self.settings.clone()),
            SettingKind::Applications => {
                EngineCommand::UpdateApplicationColorRules(self.settings.application_colors.clone())
            }
            SettingKind::Ui => {
                self.schedule_settings_save();
                return;
            }
        };
        self.queue(c);
        self.schedule_settings_save()
    }
    pub fn resolve_new_session(&mut self, o: crate::app::commands::NewSessionOutcome) {
        self.pending_new_session_decision = false;
        match o {
            crate::app::commands::NewSessionOutcome::ClearPreviousCanvas => {
                self.start_after_clear = true;
                self.queue_transition(
                    EngineCommand::Clear(TransitionRequest::new(())),
                    TransitionKind::Clear,
                    RecordingStatus::Stopped,
                );
            }
            crate::app::commands::NewSessionOutcome::PreserveForExport
            | crate::app::commands::NewSessionOutcome::Cancel => {}
        }
    }
    pub fn export_and_start_new_session(&mut self) {
        let request = self.export_request();
        self.export_start_new = Some(request.id);
        self.queue(EngineCommand::RequestExport(request));
    }
    pub fn request_start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Finished {
            self.pending_new_session_decision = true;
            return;
        }
        self.automatic_start_pending = true
    }
    pub fn apply_command(&mut self, c: crate::app::commands::AppCommand) {
        use crate::app::commands::AppCommand::*;
        let policy = crate::app::action_policy::action_policy(
            self.recording_status,
            self.pending_transition.is_some(),
            self.export_busy(),
            !self.preview.is_empty(),
            self.capture_health.as_ref().is_none_or(|health| {
                health.engine == crate::session::snapshot::EngineConnectionState::Connected
            }),
            false,
        );
        let allowed = match c {
            StartRecording => policy.start.enabled,
            PauseRecording | ResumeRecording | TogglePauseResume => policy.pause_resume.enabled,
            FinishSession => policy.finish.enabled,
            ExportCurrentCanvas => policy.export.enabled,
            Show | MinimizeToTray | Exit | ExitFromTray => true,
        };
        if !allowed {
            self.status_message =
                Some("That action is not available in the confirmed engine state.".into());
            return;
        }
        match c {
            StartRecording => self.request_start_recording(),
            PauseRecording => self.queue_transition(
                EngineCommand::Pause(TransitionRequest::new(())),
                TransitionKind::Pause,
                RecordingStatus::Paused,
            ),
            ResumeRecording => self.queue_transition(
                EngineCommand::Resume(TransitionRequest::new(())),
                TransitionKind::Resume,
                RecordingStatus::Recording,
            ),
            TogglePauseResume => {
                let (c, k, s) = if self.recording_status == RecordingStatus::Paused {
                    (
                        EngineCommand::Resume(TransitionRequest::new(())),
                        TransitionKind::Resume,
                        RecordingStatus::Recording,
                    )
                } else {
                    (
                        EngineCommand::Pause(TransitionRequest::new(())),
                        TransitionKind::Pause,
                        RecordingStatus::Paused,
                    )
                };
                self.queue_transition(c, k, s)
            }
            FinishSession => self.queue_transition(
                EngineCommand::Finish(TransitionRequest::new(())),
                TransitionKind::Finish,
                RecordingStatus::Finished,
            ),
            ExportCurrentCanvas => {
                let request = self.export_request();
                self.queue(EngineCommand::RequestExport(request));
            }
            MinimizeToTray => self.minimize_requested = true,
            Show | Exit | ExitFromTray => {}
        }
    }
}
impl AppState {
    pub fn export_request(&self) -> crate::session::events::ExportRequest {
        use crate::export::model::{
            ExportBackground, ExportDestination, ExportFormat, ExportScale, InformationPanels,
        };
        let scale = match self.settings.export_scale {
            v if v <= 0.375 => ExportScale::TwentyFive,
            v if v <= 0.625 => ExportScale::Fifty,
            v if v <= 0.875 => ExportScale::SeventyFive,
            _ => ExportScale::Full,
        };
        let background = match self.settings.export_background_mode {
            crate::settings::model::ExportBackgroundMode::Transparent => {
                ExportBackground::Transparent
            }
            _ => ExportBackground::Solid(self.settings.background_color.clone()),
        };
        crate::session::events::ExportRequest {
            id: crate::session::events::ExportRequestId::next(),
            destination: ExportDestination::Directory(self.settings.export_directory.clone()),
            format: ExportFormat::Png,
            timestamp: std::time::SystemTime::now(),
            scale,
            background,
            panels: InformationPanels {
                monitor_outlines: self.settings.export_monitor_overlays,
                monitor_labels: self.settings.export_monitor_overlays,
                ..Default::default()
            },
        }
    }
    pub fn retry_export(&mut self) {
        let retry = self.export_result.as_ref().and_then(|r| {
            if let crate::session::events::ExportResult::Failure { retry_request, .. } = r {
                Some(retry_request.clone())
            } else {
                None
            }
        });
        if let Some(mut request) = retry {
            request.id = crate::session::events::ExportRequestId::next();
            self.export_error = None;
            self.queue(EngineCommand::RequestExport(request));
        }
    }
}
#[derive(Debug, Clone)]
pub struct PendingTransition {
    pub request_id: TransitionRequestId,
    pub kind: TransitionKind,
    pub submitted_at: Instant,
    pub expected_status: RecordingStatus,
}
#[derive(Clone, Copy)]
enum SettingKind {
    Recording,
    Drawing,
    Background,
    Applications,
    Ui,
}
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsUpdate {
    SamplingIntervalMs(u64),
    MovementThresholdPx(f32),
    DwellToleranceRadiusPx(f32),
    DwellActivationDelayMs(u64),
    TransparentCanvasMode(bool),
    MonitorOutlines(bool),
    MonitorLabels(bool),
    PreviewFitBehavior(crate::settings::model::PreviewFitBehavior),
    LineWidthPx(f32),
    LineOpacity(f32),
    DwellShapeKind(crate::settings::model::DwellShapeKind),
    MinDwellSize(f32),
    MaxDwellSize(f32),
    DwellGrowthRate(f32),
    DwellFillOpacity(f32),
    DwellOutlineWidth(f32),
    DwellRenderMode(crate::settings::model::DwellRenderMode),
    CanvasVisuals,
    AppColoringEnabled(bool),
    AppRuleColor(String, crate::settings::model::RgbaColor),
    AppRuleRename(String, String),
    AppRuleMerge { survivor: String, merged: String },
}
impl SettingsUpdate {
    fn kind(&self) -> SettingKind {
        match self {
            Self::SamplingIntervalMs(_)
            | Self::MovementThresholdPx(_)
            | Self::DwellToleranceRadiusPx(_)
            | Self::DwellActivationDelayMs(_) => SettingKind::Recording,
            Self::AppColoringEnabled(_)
            | Self::AppRuleColor(_, _)
            | Self::AppRuleRename(_, _)
            | Self::AppRuleMerge { .. } => SettingKind::Applications,
            Self::TransparentCanvasMode(_) | Self::CanvasVisuals => SettingKind::Background,
            Self::MonitorOutlines(_) | Self::MonitorLabels(_) | Self::PreviewFitBehavior(_) => {
                SettingKind::Ui
            }
            _ => SettingKind::Drawing,
        }
    }
    fn apply(&self, s: &mut crate::settings::model::AppSettings) {
        match self {
            Self::SamplingIntervalMs(v) => s.sampling_interval_ms = *v,
            Self::MovementThresholdPx(v) => s.movement_threshold_px = *v,
            Self::DwellToleranceRadiusPx(v) => s.dwell_tolerance_radius_px = *v,
            Self::DwellActivationDelayMs(v) => s.dwell_activation_delay_ms = *v,
            Self::TransparentCanvasMode(v) => s.transparent_canvas_mode = *v,
            Self::MonitorOutlines(v) => s.preview_options.monitor_outlines = *v,
            Self::MonitorLabels(v) => s.preview_options.monitor_labels = *v,
            Self::PreviewFitBehavior(v) => s.preview_fit_behavior = *v,
            Self::LineWidthPx(v) => s.line_width_px = *v,
            Self::LineOpacity(v) => s.line_opacity = *v,
            Self::DwellShapeKind(v) => s.selected_dwell_shape = *v,
            Self::MinDwellSize(v) => s.min_dwell_shape_size = *v,
            Self::MaxDwellSize(v) => s.max_dwell_shape_size = *v,
            Self::DwellGrowthRate(v) => s.dwell_growth_rate = *v,
            Self::DwellFillOpacity(v) => s.dwell_fill_opacity = *v,
            Self::DwellOutlineWidth(v) => s.dwell_outline_width = *v,
            Self::DwellRenderMode(v) => s.dwell_render_mode = *v,
            Self::CanvasVisuals => {}
            Self::AppColoringEnabled(v) => s.app_specific_coloring_enabled = *v,
            Self::AppRuleColor(id, c) => {
                s.application_colors
                    .set_manual_override_by_rule_id(id, c.clone());
            }
            Self::AppRuleRename(id, l) => {
                let _ = s.application_colors.rename_rule(id, l.clone());
            }
            Self::AppRuleMerge { survivor, merged } => {
                let _ = s.application_colors.merge_rules(survivor, merged);
            }
        }
    }
}
