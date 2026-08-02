use crate::app::{commands::AppCommand, state::AppState};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let policy = crate::app::action_policy::action_policy(
            state.recording_status,
            state.pending_transition.is_some(),
            state.export_busy(),
            !state.preview.is_empty(),
            state.capture_health.as_ref().is_none_or(|h| {
                h.engine == crate::session::snapshot::EngineConnectionState::Connected
            }),
            false,
        );
        ui.heading("MultiMouseCanvas");
        if ui
            .add_enabled(policy.start.enabled, egui::Button::new(policy.start.label))
            .clicked()
        {
            state.apply_command(AppCommand::StartRecording);
        }
        if ui
            .add_enabled(
                policy.pause_resume.enabled,
                egui::Button::new(policy.pause_resume.label),
            )
            .clicked()
        {
            state.apply_command(AppCommand::TogglePauseResume);
        }
        if ui
            .add_enabled(
                policy.finish.enabled,
                egui::Button::new(policy.finish.label),
            )
            .clicked()
        {
            state.apply_command(AppCommand::FinishSession);
        }
        if ui
            .add_enabled(
                policy.export.enabled,
                egui::Button::new(policy.export.label),
            )
            .clicked()
        {
            state.apply_command(AppCommand::ExportCurrentCanvas);
        }
        if ui
            .add_enabled(policy.clear.enabled, egui::Button::new(policy.clear.label))
            .clicked()
        {
            state.request_clear_canvas_confirmation();
        }
        let response = ui.add_enabled(state.tray_available, egui::Button::new("Minimize to tray"));
        let response = if state.tray_available {
            response
        } else {
            response.on_disabled_hover_text(state.tray_error.as_deref().unwrap_or(
                "The system tray could not be initialized; the only window will remain visible.",
            ))
        };
        if response.clicked() {
            state.apply_command(AppCommand::MinimizeToTray);
        }
    });
}
