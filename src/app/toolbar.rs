use crate::{
    app::{commands::AppCommand, state::AppState},
    session::model::RecordingStatus,
};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.heading("MultiMouseCanvas");
        if ui
            .add_enabled(
                state.recording_status == RecordingStatus::Stopped,
                egui::Button::new("Start"),
            )
            .clicked()
        {
            state.apply_command(AppCommand::StartRecording);
        }
        let pause = if state.recording_status == RecordingStatus::Paused {
            "Resume"
        } else {
            "Pause"
        };
        if ui
            .add_enabled(
                state.recording_status != RecordingStatus::Stopped,
                egui::Button::new(pause),
            )
            .clicked()
        {
            state.apply_command(AppCommand::TogglePauseResume);
        }
        if ui
            .add_enabled(
                state.recording_status != RecordingStatus::Stopped,
                egui::Button::new("Finish"),
            )
            .clicked()
        {
            state.apply_command(AppCommand::FinishSession);
        }
        if ui
            .add_enabled(!state.preview.is_empty(), egui::Button::new("Export PNG"))
            .clicked()
        {
            state.apply_command(AppCommand::ExportCurrentCanvas);
        }
        if ui.button("Clear").clicked() {
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
