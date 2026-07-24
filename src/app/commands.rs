use super::state::AppState;
use crate::{
    export::image_export::{export_image, ExportBackground, ExportFormat, ExportOptions},
    session::{
        manifest::{create_session_directory, SessionManifest, RECOVERY_SCHEMA_VERSION},
        model::RecordingStatus,
        recovery::{self},
    },
};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    Show,
    StartRecording,
    PauseRecording,
    ResumeRecording,
    TogglePauseResume,
    FinishSession,
    ExportCurrentCanvas,
    Exit,
}

impl AppCommand {
    pub fn wire_sources() -> &'static [&'static str] {
        &["ui", "tray", "cli"]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    UnknownArgument(String),
    HelpRequested,
}

pub fn cli_help_text() -> &'static str {
    "MultiMouseCanvas command-line commands:\n\
     --show      Show the application window\n\
     --start     Start recording global mouse position samples\n\
     --pause     Pause recording without collecting mouse samples\n\
     --resume    Resume recording mouse position samples\n\
     --finish    Finish the session and stop recording\n\
     --help      Print this help text\n\n\
     Privacy: commands control mouse-position recording only; they do not collect clicks, keyboard input, screenshots, window contents, browser URLs, or window titles by default."
}

pub fn parse_cli_args<I, S>(args: I) -> Result<Vec<AppCommand>, CliParseError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|arg| match arg.as_ref() {
            "--start" => Ok(AppCommand::StartRecording),
            "--show" => Ok(AppCommand::Show),
            "--pause" => Ok(AppCommand::PauseRecording),
            "--resume" => Ok(AppCommand::ResumeRecording),
            "--finish" => Ok(AppCommand::FinishSession),
            "--help" | "-h" => Err(CliParseError::HelpRequested),
            other => Err(CliParseError::UnknownArgument(other.to_owned())),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSessionOutcome {
    ClearPreviousCanvas,
    PreserveForExport,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CloseWindowBehavior {
    MinimizeToTrayWhileRecording,
    ExitAfterConfirmation,
    AlwaysExit,
}
impl Default for CloseWindowBehavior {
    fn default() -> Self {
        Self::MinimizeToTrayWhileRecording
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseWindowAction {
    HideToTray,
    AskForExitConfirmation,
    Exit,
}

pub fn resolve_close_window_action(
    behavior: CloseWindowBehavior,
    status: RecordingStatus,
) -> CloseWindowAction {
    match behavior {
        CloseWindowBehavior::MinimizeToTrayWhileRecording if status != RecordingStatus::Stopped => {
            CloseWindowAction::HideToTray
        }
        CloseWindowBehavior::MinimizeToTrayWhileRecording => CloseWindowAction::Exit,
        CloseWindowBehavior::ExitAfterConfirmation => CloseWindowAction::AskForExitConfirmation,
        CloseWindowBehavior::AlwaysExit => CloseWindowAction::Exit,
    }
}

impl AppState {
    pub fn apply_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::Show => self.status_message = Some("Window shown.".to_owned()),
            AppCommand::StartRecording => self.request_start_recording(),
            AppCommand::PauseRecording => self.pause_recording(),
            AppCommand::ResumeRecording => self.resume_recording(),
            AppCommand::TogglePauseResume => self.toggle_pause_resume(),
            AppCommand::FinishSession => self.finish_session(),
            AppCommand::ExportCurrentCanvas => self.export_canvas_to_default(),
            AppCommand::Exit => self.exit_requested = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cli_arguments_into_app_commands() {
        assert_eq!(
            parse_cli_args(["--start", "--show", "--pause", "--resume", "--finish"]).unwrap(),
            vec![
                AppCommand::StartRecording,
                AppCommand::Show,
                AppCommand::PauseRecording,
                AppCommand::ResumeRecording,
                AppCommand::FinishSession
            ]
        );
        assert_eq!(
            parse_cli_args(["--bad"]),
            Err(CliParseError::UnknownArgument("--bad".into()))
        );
    }
    #[test]
    fn ui_tray_and_cli_share_command_enum() {
        assert_eq!(AppCommand::wire_sources(), &["ui", "tray", "cli"]);
    }

    #[test]
    fn cli_help_documents_privacy_sensitive_recording_commands() {
        let help = cli_help_text();
        assert!(help.contains("Start recording global mouse position samples"));
        assert!(help.contains("Pause recording without collecting mouse samples"));
        assert!(help.contains("Finish the session and stop recording"));
        assert!(help.contains("do not collect clicks"));
        assert!(help.contains("keyboard input"));
        assert!(help.contains("screenshots"));
        assert!(matches!(
            parse_cli_args(["--help"]),
            Err(CliParseError::HelpRequested)
        ));
    }
    #[test]
    fn close_behavior_resolves_for_recording_and_stopped() {
        assert_eq!(
            resolve_close_window_action(
                CloseWindowBehavior::MinimizeToTrayWhileRecording,
                RecordingStatus::Recording
            ),
            CloseWindowAction::HideToTray
        );
        assert_eq!(
            resolve_close_window_action(
                CloseWindowBehavior::MinimizeToTrayWhileRecording,
                RecordingStatus::Stopped
            ),
            CloseWindowAction::Exit
        );
        assert_eq!(
            resolve_close_window_action(
                CloseWindowBehavior::ExitAfterConfirmation,
                RecordingStatus::Recording
            ),
            CloseWindowAction::AskForExitConfirmation
        );
        assert_eq!(
            resolve_close_window_action(CloseWindowBehavior::AlwaysExit, RecordingStatus::Paused),
            CloseWindowAction::Exit
        );
    }
}

impl AppState {
    pub fn request_start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Stopped
            && self.has_unexported_canvas
            && !self.canvas.is_empty()
        {
            self.pending_new_session_decision = true;
            self.status_message = Some(
                "Previous canvas has not been exported: clear, export/preserve, or cancel."
                    .to_owned(),
            );
        } else {
            self.start_recording();
        }
    }

    pub fn resolve_new_session(&mut self, outcome: NewSessionOutcome) {
        self.pending_new_session_decision = false;
        match outcome {
            NewSessionOutcome::ClearPreviousCanvas => {
                self.clear_canvas_internal();
                self.start_recording();
            }
            NewSessionOutcome::PreserveForExport => {
                self.status_message = Some("Previous canvas preserved. Export or clear it before starting another session.".to_owned());
            }
            NewSessionOutcome::Cancel => {
                self.status_message =
                    Some("New session canceled; previous canvas preserved.".to_owned());
            }
        }
    }

    pub fn start_recording(&mut self) {
        if self.recording_status == RecordingStatus::Stopped {
            if self.has_unexported_canvas && !self.canvas.is_empty() {
                self.request_start_recording();
                return;
            }
            self.recording_status = RecordingStatus::Recording;
            self.mark_started_now();
            if let Some(root) = self.recovery_path.clone() {
                if root.file_name().and_then(|n| n.to_str()) == Some("recovery") {
                    match create_session_directory(&root, SystemTime::now()) {
                        Ok((_id, dir)) => self.recovery_path = Some(dir),
                        Err(e) => {
                            self.status_message =
                                Some(format!("Recovery directory unavailable: {e}"))
                        }
                    }
                }
            }
            self.movement_classifier =
                crate::session::controller::MovementClassifier::new(&self.settings);
            self.start_sampler();
            self.autosave_recovery(false);
            self.status_message = Some("Recording started.".to_owned());
        }
    }

    pub fn pause_recording(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.recording_status = RecordingStatus::Paused;
            self.stop_sampler();
            self.movement_classifier
                .mark_discontinuity(crate::session::controller::DiscontinuityReason::PauseResume);
            self.autosave_recovery(false);
            self.status_message = Some("Recording paused.".to_owned());
        }
    }

    pub fn resume_recording(&mut self) {
        if self.recording_status == RecordingStatus::Paused {
            self.recording_status = RecordingStatus::Recording;
            self.movement_classifier
                .mark_discontinuity(crate::session::controller::DiscontinuityReason::PauseResume);
            self.start_sampler();
            self.autosave_recovery(false);
            self.status_message = Some("Recording resumed.".to_owned());
        }
    }

    pub fn toggle_pause_resume(&mut self) {
        match self.recording_status {
            RecordingStatus::Recording => self.pause_recording(),
            RecordingStatus::Paused => self.resume_recording(),
            RecordingStatus::Stopped => {}
        }
    }

    pub fn finish_session(&mut self) {
        if matches!(
            self.recording_status,
            RecordingStatus::Recording | RecordingStatus::Paused
        ) {
            self.stop_sampler();
            self.movement_classifier.finalize_active_segment();
            self.movement_classifier.finalize_dwell();
            self.recording_status = RecordingStatus::Stopped;
            self.sync_retained_canvas_and_statistics();
            self.timing.started_at = None;
            self.has_unexported_canvas = !self.canvas.is_empty();
            self.autosave_recovery(true);
            self.status_message = Some(format!(
                "Session finished. Samples: {}, movements: {}, dwells: {}. Export is available.",
                self.statistics.samples_recorded,
                self.statistics.movement_segment_count,
                self.statistics.finalized_dwell_count
            ));
        }
    }

    pub fn export_canvas_to_default(&mut self) {
        let mut options = ExportOptions::basic(self.settings.export_directory.clone());
        options.timestamp = SystemTime::now();
        options.format = ExportFormat::Png;
        options.background = if self.settings.transparent_canvas_mode {
            ExportBackground::Transparent
        } else {
            ExportBackground::Solid(self.settings.background_color.clone())
        };
        if self.export_busy {
            return;
        }
        let canvas = self.canvas.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.export_rx = Some(rx);
        self.export_busy = true;
        self.export_progress = 0.05;
        self.status_message = Some("Export running in background…".into());
        std::thread::spawn(move || {
            let _ = tx.send(export_image(&canvas, &options).map_err(|e| e.to_string()));
        });
    }

    pub fn clear_canvas_when_safe(&mut self) {
        if self.recording_status == RecordingStatus::Recording {
            self.status_message =
                Some("Pause or finish recording before clearing the canvas.".to_owned());
            return;
        }
        if self.has_unexported_canvas && !self.canvas.is_empty() {
            self.status_message = Some("Canvas has unexported data. Choose clear previous canvas, preserve/export, or cancel new session.".to_owned());
            return;
        }
        self.clear_canvas_internal();
        self.status_message = Some("Canvas and statistics cleared.".to_owned());
    }

    pub fn export_and_start_new_session(&mut self) {
        if self.export_busy {
            return;
        }
        self.pending_new_session_decision = false;
        self.export_start_new = true;
        self.export_canvas_to_default();
    }

    pub(crate) fn clear_canvas_internal(&mut self) {
        self.canvas.clear();
        self.statistics.reset();
        self.has_unexported_canvas = false;
        self.movement_classifier =
            crate::session::controller::MovementClassifier::new(&self.settings);
    }

    pub fn autosave_recovery(&mut self, completed: bool) {
        self.sync_retained_canvas_and_statistics();
        if let Some(path) = &self.recovery_path {
            let manifest = SessionManifest {
                schema_version: RECOVERY_SCHEMA_VERSION,
                session_id: path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("session")
                    .to_owned(),
                started_at: self.timing.started_at.unwrap_or(SystemTime::UNIX_EPOCH),
                saved_at: SystemTime::now(),
                completed,
                recording_status: self.recording_status,
                session_bounds: self.canvas.session_desktop_bounds,
                current_topology: self.canvas.current_topology.clone(),
                topology_history: self.canvas.topology_history.clone(),
                statistics: self.statistics.clone(),
                background: self.canvas.background.clone(),
                tile_size: self.canvas.sparse_tiles.tile_size,
                pixel_format: "RGBA8".into(),
                application_colors: self.settings.application_colors.clone(),
                tiles: self
                    .canvas
                    .sparse_tiles
                    .tiles
                    .keys()
                    .map(|c| recovery::tile_filename(*c))
                    .collect(),
            };
            if let Err(e) = recovery::save_session(path, &manifest, &mut self.canvas.sparse_tiles) {
                tracing::warn!(%e, "non-fatal recovery save failed");
                self.status_message =
                    Some(format!("Recovery save failed; recording continues: {e}"));
            }
        }
    }

    pub fn restore_recovery(&mut self) {
        if let Some(path) = &self.recovery_path {
            if let Ok((m, canvas)) = recovery::restore_canvas(path) {
                self.canvas = canvas;
                self.statistics = m.statistics;
                self.settings.application_colors = m.application_colors;
                self.recording_status = RecordingStatus::Stopped;
                self.movement_classifier.mark_discontinuity(
                    crate::session::controller::DiscontinuityReason::PauseResume,
                );
                self.has_unexported_canvas = true;
                self.status_message = Some("Recovered incomplete session canvas.".to_owned());
            }
        }
    }
    pub fn discard_recovery(&mut self) {
        if let Some(path) = &self.recovery_path {
            let _ = recovery::discard_recovery(path);
            self.status_message = Some("Recovery data discarded.".to_owned());
        }
    }
}
