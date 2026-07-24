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
            .add_enabled(!state.canvas.is_empty(), egui::Button::new("Export PNG"))
            .clicked()
        {
            state.apply_command(AppCommand::ExportCurrentCanvas);
        }
        if ui.button("Clear").clicked() {
            state.request_clear_canvas_confirmation();
        }
    });
}
