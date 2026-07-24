pub mod application_editor;
pub mod commands;
pub mod dialogs;
pub mod performance_view;
pub mod settings_view;
pub mod state;
pub mod toolbar;
pub mod view;

use crate::app::commands::{resolve_close_window_action, AppCommand, CloseWindowAction};
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
        self.state.flush_settings_save_if_due();
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
        view::show(ctx, &mut self.state, &mut self.confirm_exit);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.stop_sampler();
        self.state.save_settings_as_status();
    }
}
