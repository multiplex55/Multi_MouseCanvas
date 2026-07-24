use crate::{
    canvas::{
        coordinates::CanvasPoint,
        model::{CanvasModel, DwellShape, MovementPath},
        rasterizer::{rasterize_dwell_shape, rasterize_movement_path},
    },
    capture::{
        foreground::{ForegroundApplication, ForegroundResolver},
        sampler::{CursorSample, CursorSampler},
        windows::WindowsPollingSampler,
    },
    session::{
        controller::MovementClassifier,
        model::{RecordingStatus, SessionTiming},
        statistics::SessionStatistics,
    },
    settings::{model::AppSettings, storage},
};
use std::{
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, Instant, SystemTime},
};

pub struct AppState {
    pub recording_status: RecordingStatus,
    pub timing: SessionTiming,
    pub canvas: CanvasModel,
    pub current_cursor_sample: Option<CursorSample>,
    pub movement_classifier: MovementClassifier,
    sampler: Option<Box<dyn CursorSampler>>,
    sample_rx: Option<Receiver<CursorSample>>,
    pub current_foreground_application: ForegroundApplication,
    foreground_resolver: Option<Box<dyn ForegroundResolver>>,
    pub statistics: SessionStatistics,
    pub settings: AppSettings,
    pub status_message: Option<String>,
    pub has_unexported_canvas: bool,
    pub pending_new_session_decision: bool,
    pub recovery_path: Option<PathBuf>,
    pub samples_since_autosave: u64,
    last_topology_refresh: Instant,
    pub settings_path: Option<PathBuf>,
    pub exit_requested: bool,
    pub pending_settings_save: Option<Instant>,
    pub emitted_settings_commands: Vec<EngineSettingsCommand>,
    pub lifecycle_dialogs: crate::app::dialogs::LifecycleDialogState,
    pub performance_diagnostics: crate::app::performance_view::PerformanceDiagnostics,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            recording_status: RecordingStatus::Stopped,
            timing: SessionTiming::default(),
            canvas: CanvasModel::default(),
            current_cursor_sample: None,
            movement_classifier: MovementClassifier::new(&AppSettings::default()),
            sampler: None,
            sample_rx: None,
            current_foreground_application: ForegroundApplication::default(),
            foreground_resolver: None,
            statistics: SessionStatistics::default(),
            settings: AppSettings::default(),
            status_message: None,
            has_unexported_canvas: false,
            pending_new_session_decision: false,
            recovery_path: None,
            samples_since_autosave: 0,
            last_topology_refresh: Instant::now(),
            settings_path: None,
            exit_requested: false,
            pending_settings_save: None,
            emitted_settings_commands: Vec::new(),
            lifecycle_dialogs: crate::app::dialogs::LifecycleDialogState::default(),
            performance_diagnostics: crate::app::performance_view::PerformanceDiagnostics::default(
            ),
        }
    }
}

impl AppState {
    pub fn load() -> Self {
        let mut state = Self::default();
        match storage::default_settings_path() {
            Ok(path) => {
                state.settings_path = Some(path.clone());
                match storage::load_or_default(&path) {
                    Ok(mut settings) => {
                        settings.validate();
                        state.settings = settings;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "settings load failed; using defaults");
                        state.status_message =
                            Some(format!("Settings load failed; using defaults: {error}"));
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "settings path unavailable; using defaults");
                state.status_message = Some(format!(
                    "Settings path unavailable; using defaults: {error}"
                ));
            }
        }
        state.movement_classifier = MovementClassifier::new(&state.settings);
        if let Ok(path) = storage::default_settings_path() {
            let recovery_path = path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("recovery")
                .join("autosave.recovery.json");
            if matches!(
                crate::session::recovery::detect_incomplete(&recovery_path),
                crate::session::recovery::RecoveryStatus::Incomplete(_)
            ) {
                state.status_message = Some(
                    "Incomplete recovery data found. Restore or discard it before recording."
                        .to_owned(),
                );
            }
            state.recovery_path = Some(recovery_path);
        }
        if state.settings.start_recording_automatically {
            state.start_recording();
        }
        state
    }

    pub fn mark_started_now(&mut self) {
        self.timing.started_at = Some(SystemTime::now());
    }

    #[cfg(test)]
    pub fn install_sampler_for_tests(&mut self, sampler: Box<dyn CursorSampler>) {
        self.sampler = Some(sampler);
    }

    pub fn start_sampler(&mut self) {
        if self.sampler.is_none() {
            self.sampler = Some(Box::new(WindowsPollingSampler::new(
                self.settings.sampling_interval_ms,
            )));
        }
        if let Some(sampler) = &mut self.sampler {
            self.sample_rx = Some(sampler.start());
        }
    }

    pub fn stop_sampler(&mut self) {
        if let Some(sampler) = &mut self.sampler {
            sampler.stop();
        }
        self.sample_rx = None;
    }

    #[cfg(test)]
    pub fn install_foreground_resolver_for_tests(&mut self, resolver: Box<dyn ForegroundResolver>) {
        self.foreground_resolver = Some(resolver);
    }

    fn resolve_foreground_for_sample(&mut self) -> ForegroundApplication {
        let app = match self.foreground_resolver.as_mut() {
            Some(resolver) => resolver
                .resolve_foreground()
                .unwrap_or_else(|_| ForegroundApplication::unknown()),
            None => crate::capture::windows::resolve_foreground_application()
                .unwrap_or_else(|_| ForegroundApplication::unknown()),
        };
        self.current_foreground_application = app.clone();
        app
    }

    pub fn drain_samples(&mut self) {
        let mut drained = Vec::new();
        if let Some(rx) = &self.sample_rx {
            while let Ok(sample) = rx.try_recv() {
                drained.push(sample);
            }
        }
        self.refresh_display_topology_if_due();
        for sample in drained {
            self.statistics.samples_recorded += 1;
            self.current_cursor_sample = Some(sample.clone());
            let app = self.resolve_foreground_for_sample();
            let color = if self.settings.app_specific_coloring_enabled {
                self.settings
                    .application_colors
                    .color_for(&app.identity, &self.settings.default_movement_color)
            } else {
                self.settings.default_movement_color.clone()
            };
            self.movement_classifier
                .set_foreground_context(app.identity, color);
            self.movement_classifier.accept_sample(sample);
            self.sync_retained_canvas_and_statistics();
            self.samples_since_autosave += 1;
            if self.samples_since_autosave >= 60 {
                self.autosave_recovery(false);
                self.samples_since_autosave = 0;
            }
        }
    }

    fn refresh_display_topology_if_due(&mut self) {
        if self.last_topology_refresh.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_topology_refresh = Instant::now();
        let Ok(topology) = crate::platform::display::current_topology() else {
            return;
        };
        if topology.signature != self.canvas.current_topology.signature {
            if let Some(path) = self.canvas.active_movement_overlay.take() {
                rasterize_movement_path(&mut self.canvas.sparse_tiles, &path);
            }
            if let Some(shape) = self.canvas.active_dwell_overlay.take() {
                rasterize_dwell_shape(&mut self.canvas.sparse_tiles, &shape);
            }
            self.canvas
                .topology_history
                .record_if_changed(self.canvas.current_topology.clone());
            self.canvas.session_desktop_bounds = crate::canvas::topology::expand_session_bounds(
                self.canvas.session_desktop_bounds,
                &topology,
            );
            self.canvas.current_topology = topology.clone();
            self.canvas.topology_history.record_if_changed(topology);
            self.canvas.refresh_dimensions();
        }
    }

    pub fn sync_retained_canvas_and_statistics(&mut self) {
        self.canvas.background.color = self.settings.background_color.clone();
        self.canvas.background.transparent = self.settings.transparent_canvas_mode;

        let segments_len = self.movement_classifier.segments.len();
        for segment in self
            .movement_classifier
            .segments
            .iter()
            .skip(self.canvas.committed_movement_count)
        {
            let path = self.path_from_segment(segment, true);
            rasterize_movement_path(&mut self.canvas.sparse_tiles, &path);
            self.canvas.tile_generation += 1;
        }
        self.canvas.committed_movement_count = segments_len;
        self.canvas.active_movement_overlay = self
            .movement_classifier
            .active_segment()
            .map(|segment| self.path_from_segment(segment, false));

        let dwells_len = self.movement_classifier.dwells.len();
        for dwell in self
            .movement_classifier
            .dwells
            .iter()
            .skip(self.canvas.committed_dwell_count)
        {
            let shape = self.dwell_shape_from_event(dwell, true);
            rasterize_dwell_shape(&mut self.canvas.sparse_tiles, &shape);
            self.canvas.tile_generation += 1;
        }
        self.canvas.committed_dwell_count = dwells_len;
        self.canvas.active_dwell_overlay = self
            .movement_classifier
            .active_dwell()
            .map(|dwell| self.dwell_shape_from_event(&dwell, false));
        self.canvas.refresh_dimensions();

        self.statistics.total_cursor_distance = self.movement_classifier.total_distance;
        self.statistics.finalized_dwell_count = self.movement_classifier.dwells.len() as u64;
        self.statistics.dwell_events = self.statistics.finalized_dwell_count;
        self.statistics.current_dwell_duration = self.movement_classifier.current_dwell_duration();
        self.statistics.longest_dwell = self
            .movement_classifier
            .dwells
            .iter()
            .map(|d| d.duration)
            .chain(std::iter::once(self.statistics.current_dwell_duration))
            .max()
            .unwrap_or(Duration::ZERO);
        self.statistics.movement_segment_count = self.movement_classifier.segments.len() as u64
            + u64::from(self.movement_classifier.active_segment().is_some());
        self.statistics.movements_recorded = self.statistics.movement_segment_count;
        self.statistics.session_duration = self
            .timing
            .started_at
            .and_then(|started| started.elapsed().ok())
            .unwrap_or(Duration::ZERO);
    }

    fn path_from_segment(
        &self,
        segment: &crate::session::controller::MovementSegment,
        finalized: bool,
    ) -> MovementPath {
        let mut path = MovementPath::new(
            segment.color.clone(),
            self.settings.line_width_px,
            finalized,
        );
        path.application = segment.application.clone();
        for (x, y) in &segment.points {
            path.push_simplified(
                CanvasPoint { x: *x, y: *y },
                self.canvas.point_merge_distance,
            );
        }
        path
    }

    fn dwell_shape_from_event(
        &self,
        dwell: &crate::session::controller::DwellEvent,
        finalized: bool,
    ) -> DwellShape {
        let mut shape = DwellShape::from_duration(
            CanvasPoint {
                x: dwell.center_x,
                y: dwell.center_y,
            },
            dwell.duration,
            dwell.color.clone(),
            self.settings.selected_dwell_shape,
            self.settings.min_dwell_shape_size,
            self.settings.max_dwell_shape_size,
            self.settings.dwell_growth_rate,
            self.settings.dwell_fill_opacity,
            self.settings.dwell_outline_width,
            self.settings.dwell_render_mode,
            finalized,
        );
        shape.application = dwell.application.clone();
        shape
    }

    pub fn schedule_settings_save(&mut self) {
        self.pending_settings_save = Some(Instant::now() + Duration::from_millis(350));
    }

    pub fn flush_settings_save_if_due(&mut self) {
        if self
            .pending_settings_save
            .is_some_and(|due| Instant::now() >= due)
        {
            self.pending_settings_save = None;
            self.save_settings_as_status();
        }
    }

    pub fn request_clear_canvas_confirmation(&mut self) {
        self.lifecycle_dialogs
            .request_clear(!self.canvas.is_empty());
        if self.canvas.is_empty() {
            self.confirm_clear_canvas();
        }
    }
    pub fn confirm_clear_canvas(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.status_message =
                Some("Pause or finish recording before clearing the canvas.".to_owned());
            return;
        }
        self.clear_canvas_internal();
        if let Some(path) = &self.recovery_path {
            let _ = crate::session::recovery::discard_recovery(path);
        }
        self.lifecycle_dialogs.clear_confirmation_open = false;
        self.status_message =
            Some("Canvas artwork and recovery cleared; application colors kept.".to_owned());
    }

    pub fn apply_settings_update(&mut self, update: SettingsUpdate) {
        update.apply(&mut self.settings);
        self.settings.validate();
        self.movement_classifier.update_settings(&self.settings);
        self.canvas.background.color = self.settings.background_color.clone();
        self.canvas.background.transparent = self.settings.transparent_canvas_mode;
        self.emitted_settings_commands
            .push(update.to_engine_command());
        self.schedule_settings_save();
    }

    pub fn save_settings_as_status(&mut self) {
        if let Some(path) = &self.settings_path {
            if let Err(error) = storage::save(path, &self.settings) {
                tracing::warn!(%error, "settings save failed");
                self.status_message = Some(format!("Settings save failed: {error}"));
            }
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.stop_sampler();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineSettingsCommand {
    RecordingConfigChanged,
    VisualConfigChanged,
    ApplicationColorRuleChanged,
    ExportConfigChanged,
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
    fn to_engine_command(&self) -> EngineSettingsCommand {
        match self {
            Self::SamplingIntervalMs(_)
            | Self::MovementThresholdPx(_)
            | Self::DwellToleranceRadiusPx(_)
            | Self::DwellActivationDelayMs(_) => EngineSettingsCommand::RecordingConfigChanged,
            Self::AppColoringEnabled(_)
            | Self::AppRuleColor(_, _)
            | Self::AppRuleRename(_, _)
            | Self::AppRuleMerge { .. } => EngineSettingsCommand::ApplicationColorRuleChanged,
            Self::CanvasVisuals
            | Self::TransparentCanvasMode(_)
            | Self::MonitorOutlines(_)
            | Self::MonitorLabels(_)
            | Self::PreviewFitBehavior(_)
            | Self::LineWidthPx(_)
            | Self::LineOpacity(_)
            | Self::DwellShapeKind(_)
            | Self::MinDwellSize(_)
            | Self::MaxDwellSize(_)
            | Self::DwellGrowthRate(_)
            | Self::DwellFillOpacity(_)
            | Self::DwellOutlineWidth(_)
            | Self::DwellRenderMode(_) => EngineSettingsCommand::VisualConfigChanged,
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
#[cfg(test)]
mod state_update_tests {
    use super::*;
    #[test]
    fn settings_updates_emit_typed_engine_commands() {
        let mut s = AppState::default();
        s.apply_settings_update(SettingsUpdate::SamplingIntervalMs(0));
        assert_eq!(s.settings.sampling_interval_ms, 1);
        assert_eq!(
            s.emitted_settings_commands.last(),
            Some(&EngineSettingsCommand::RecordingConfigChanged)
        );
        assert!(s.pending_settings_save.is_some());
    }
}
