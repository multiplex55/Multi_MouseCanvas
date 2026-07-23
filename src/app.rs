pub mod commands;
pub mod state;

use crate::canvas::renderer;
use crate::session::model::RecordingStatus;
use eframe::egui;
use state::AppState;

pub struct MultiMouseCanvasApp {
    state: AppState,
}

impl MultiMouseCanvasApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::load(),
        }
    }
}

impl eframe::App for MultiMouseCanvasApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MultiMouseCanvas");
            ui.label("Windows-first multi-mouse canvas recorder");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(format!(
                    "Recording status: {:?}",
                    self.state.recording_status
                ));
                if let Some(message) = &self.state.status_message {
                    ui.colored_label(egui::Color32::YELLOW, message);
                }
            });

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.state.recording_status == RecordingStatus::Stopped,
                        egui::Button::new("Start recording"),
                    )
                    .clicked()
                {
                    self.state.start_recording();
                }

                let pause_label = match self.state.recording_status {
                    RecordingStatus::Paused => "Resume",
                    _ => "Pause",
                };
                if ui
                    .add_enabled(
                        self.state.recording_status != RecordingStatus::Stopped,
                        egui::Button::new(pause_label),
                    )
                    .clicked()
                {
                    self.state.toggle_pause_resume();
                }

                if ui
                    .add_enabled(
                        self.state.recording_status != RecordingStatus::Stopped,
                        egui::Button::new("Finish session"),
                    )
                    .clicked()
                {
                    self.state.finish_session();
                }

                if ui.button("Clear canvas").clicked() {
                    self.state.clear_canvas_when_safe();
                }

                ui.add_enabled(false, egui::Button::new("Export image (coming soon)"));
            });

            ui.collapsing("Settings", |ui| {
                ui.label(format!(
                    "Sampling interval: {} ms",
                    self.state.settings.sampling_interval_ms
                ));
                ui.label(format!(
                    "Movement threshold: {:.1} px",
                    self.state.settings.movement_threshold_px
                ));
                ui.label(format!(
                    "Dwell activation delay: {} ms",
                    self.state.settings.dwell_activation_delay_ms
                ));
                ui.label(format!(
                    "Export directory: {}",
                    self.state.settings.export_directory.display()
                ));
            });

            ui.separator();
            ui.heading("Canvas preview");
            renderer::render_preview(
                ui,
                &self.state.canvas,
                (&self.state.settings.background_color).into(),
            );

            ui.separator();
            ui.heading("Session statistics");
            ui.columns(3, |columns| {
                columns[0].label(format!(
                    "Samples: {}",
                    self.state.statistics.samples_recorded
                ));
                columns[1].label(format!(
                    "Movements: {}",
                    self.state.statistics.movements_recorded
                ));
                columns[2].label(format!(
                    "Dwell events: {}",
                    self.state.statistics.dwell_events
                ));
            });

            ui.collapsing("Capture placeholders", |ui| {
                ui.label(match &self.state.current_cursor_sample {
                    Some(sample) => format!(
                        "Cursor sample: ({:.1}, {:.1})",
                        sample.screen_x, sample.screen_y
                    ),
                    None => "Cursor sample: none".to_owned(),
                });
                ui.label(format!(
                    "Dwell active: {}",
                    self.state.current_dwell_state.is_dwelling
                ));
                ui.label(format!(
                    "Foreground application: {}",
                    self.state
                        .current_foreground_application
                        .title
                        .as_deref()
                        .unwrap_or("none")
                ));
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.save_settings_as_status();
    }
}
