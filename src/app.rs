pub mod commands;
pub mod state;

use crate::session::model::RecordingStatus;
use crate::{app_colors::registry::ApplicationColorMode, canvas::renderer};
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
        self.state.drain_samples();
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
                ui.separator();
                ui.label("Application colors");
                egui::ComboBox::from_label("Color mode")
                    .selected_text(format!("{:?}", self.state.settings.application_colors.mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.state.settings.application_colors.mode,
                            ApplicationColorMode::FixedGlobal,
                            "Fixed global color",
                        );
                        ui.selectable_value(
                            &mut self.state.settings.application_colors.mode,
                            ApplicationColorMode::ApplicationSpecific,
                            "Application-specific color",
                        );
                        ui.selectable_value(
                            &mut self.state.settings.application_colors.mode,
                            ApplicationColorMode::RandomOnce,
                            "Random once per app",
                        );
                        ui.selectable_value(
                            &mut self.state.settings.application_colors.mode,
                            ApplicationColorMode::PaletteOnce,
                            "Palette once per app",
                        );
                    });
                egui::Grid::new("app_color_editor")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Application label");
                        ui.label("Executable name/path");
                        ui.label("Assigned color");
                        ui.label("State");
                        ui.label("Action");
                        ui.end_row();
                        let keys: Vec<String> = self
                            .state
                            .settings
                            .application_colors
                            .entries
                            .keys()
                            .cloned()
                            .collect();
                        for key in keys {
                            let Some(entry) = self
                                .state
                                .settings
                                .application_colors
                                .entries
                                .get(&key)
                                .cloned()
                            else {
                                continue;
                            };
                            ui.label(&entry.label);
                            ui.label(
                                entry
                                    .executable_path
                                    .as_deref()
                                    .unwrap_or(&entry.executable_name),
                            );
                            let mut color: egui::Color32 = (&entry.resolved_color()).into();
                            let app_identity = crate::capture::foreground::ApplicationIdentity::new(
                                0,
                                entry.executable_name.clone(),
                                entry.executable_path.clone(),
                                None,
                            );
                            if ui.color_edit_button_srgba(&mut color).changed() {
                                self.state.settings.application_colors.set_manual_override(
                                    &app_identity,
                                    crate::settings::model::RgbaColor::new(
                                        color.r(),
                                        color.g(),
                                        color.b(),
                                        color.a(),
                                    ),
                                );
                            }
                            ui.label(if entry.is_manual() {
                                "Manual"
                            } else {
                                "Automatic"
                            });
                            if ui.button("Reset").clicked() {
                                self.state
                                    .settings
                                    .application_colors
                                    .reset_to_automatic(&app_identity);
                            }
                            ui.end_row();
                        }
                    });
            });

            ui.separator();
            ui.heading("Canvas preview");
            renderer::render_preview(ui, &self.state.canvas);

            ui.separator();
            ui.heading("Session statistics");
            ui.columns(3, |columns| {
                columns[0].label(format!(
                    "Samples: {}",
                    self.state.statistics.samples_recorded
                ));
                columns[0].label(format!(
                    "Session duration: {:.1}s",
                    self.state.statistics.session_duration.as_secs_f32()
                ));
                columns[1].label(format!(
                    "Distance: {:.1}px",
                    self.state.statistics.total_cursor_distance
                ));
                columns[1].label(format!(
                    "Movement segments: {}",
                    self.state.statistics.movement_segment_count
                ));
                columns[2].label(format!(
                    "Finalized dwells: {}",
                    self.state.statistics.finalized_dwell_count
                ));
                columns[2].label(format!(
                    "Current/longest dwell: {:.1}s / {:.1}s",
                    self.state.statistics.current_dwell_duration.as_secs_f32(),
                    self.state.statistics.longest_dwell.as_secs_f32()
                ));
            });

            ui.collapsing("Capture status", |ui| {
                ui.label(match &self.state.current_cursor_sample {
                    Some(sample) => format!(
                        "Cursor sample: ({:.1}, {:.1})",
                        sample.physical_x, sample.physical_y
                    ),
                    None => "Cursor sample: none".to_owned(),
                });
                ui.label(format!(
                    "Dwell visible: {}",
                    self.state.movement_classifier.current_dwell_visible()
                ));
                ui.label(format!(
                    "Foreground application: {}",
                    self.state.current_foreground_application.label()
                ));
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.stop_sampler();
        self.state.save_settings_as_status();
    }
}
