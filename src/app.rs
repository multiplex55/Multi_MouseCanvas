pub mod commands;
pub mod state;

use crate::app::commands::{resolve_close_window_action, AppCommand, CloseWindowAction};
use crate::session::model::RecordingStatus;
use crate::{app_colors::registry::ApplicationColorMode, canvas::renderer};
use eframe::egui;
use state::AppState;
use std::sync::mpsc::{self, Receiver};

pub struct MultiMouseCanvasApp {
    state: AppState,
    command_rx: Receiver<AppCommand>,
    _tray: Option<crate::tray::AppTray>,
    confirm_exit: bool,
}

impl MultiMouseCanvasApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        listener: Option<std::net::TcpListener>,
        initial_commands: Vec<AppCommand>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        if let Some(listener) = listener {
            crate::ipc::serve(listener, tx.clone());
        }
        let tray = crate::tray::AppTray::new(tx);
        let mut state = AppState::load();
        for command in initial_commands {
            state.apply_command(command);
        }
        Self {
            state,
            command_rx: rx,
            _tray: tray,
            confirm_exit: false,
        }
    }
}

impl eframe::App for MultiMouseCanvasApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(command) = self.command_rx.try_recv() {
            if command == AppCommand::Show {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
            self.state.apply_command(command);
        }
        self.state.drain_samples();
        if ctx.input(|i| i.viewport().close_requested()) {
            match resolve_close_window_action(
                self.state.settings.close_window_behavior,
                self.state.recording_status,
            ) {
                CloseWindowAction::HideToTray => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.state.status_message = Some("Recording continues in the tray.".to_owned());
                }
                CloseWindowAction::AskForExitConfirmation => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.confirm_exit = true;
                }
                CloseWindowAction::Exit => self.state.exit_requested = true,
            }
        }
        if self.state.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
                    self.state.apply_command(AppCommand::StartRecording);
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
                    self.state.apply_command(AppCommand::TogglePauseResume);
                }

                if ui
                    .add_enabled(
                        self.state.recording_status != RecordingStatus::Stopped,
                        egui::Button::new("Finish session"),
                    )
                    .clicked()
                {
                    self.state.apply_command(AppCommand::FinishSession);
                }

                if ui.button("Clear canvas").clicked() {
                    self.state.clear_canvas_when_safe();
                }

                if ui
                    .add_enabled(
                        !self.state.canvas.is_empty(),
                        egui::Button::new("Export PNG"),
                    )
                    .clicked()
                {
                    self.state.apply_command(AppCommand::ExportCurrentCanvas);
                }
            });

            if self.state.pending_new_session_decision {
                ui.horizontal(|ui| {
                    ui.label("Unexported canvas exists:");
                    if ui.button("Clear previous canvas").clicked() {
                        self.state.resolve_new_session(
                            crate::app::commands::NewSessionOutcome::ClearPreviousCanvas,
                        );
                    }
                    if ui.button("Preserve/export previous canvas").clicked() {
                        self.state.resolve_new_session(
                            crate::app::commands::NewSessionOutcome::PreserveForExport,
                        );
                    }
                    if ui.button("Cancel new session").clicked() {
                        self.state
                            .resolve_new_session(crate::app::commands::NewSessionOutcome::Cancel);
                    }
                });
            }

            ui.horizontal(|ui| {
                if ui.button("Restore recovery").clicked() {
                    self.state.restore_recovery();
                }
                if ui.button("Discard recovery").clicked() {
                    self.state.discard_recovery();
                }
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
                egui::ComboBox::from_label("Close window behavior")
                    .selected_text(format!("{:?}", self.state.settings.close_window_behavior))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.state.settings.close_window_behavior,
                            crate::app::commands::CloseWindowBehavior::MinimizeToTrayWhileRecording,
                            "Minimize to tray while recording",
                        );
                        ui.selectable_value(
                            &mut self.state.settings.close_window_behavior,
                            crate::app::commands::CloseWindowBehavior::ExitAfterConfirmation,
                            "Exit after confirmation",
                        );
                        ui.selectable_value(
                            &mut self.state.settings.close_window_behavior,
                            crate::app::commands::CloseWindowBehavior::AlwaysExit,
                            "Always exit",
                        );
                    });
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

            if self.confirm_exit {
                egui::Window::new("Exit MultiMouseCanvas?")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label("Exit and stop any background recording/sampling?");
                        ui.horizontal(|ui| {
                            if ui.button("Exit").clicked() {
                                self.state.exit_requested = true;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_exit = false;
                            }
                        });
                    });
            }

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
