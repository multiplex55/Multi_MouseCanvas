pub mod application_editor;
pub mod commands;
pub mod dialogs;
pub mod display_profile_editor;
pub mod lifecycle;
pub mod monitor_selection;
pub mod performance_view;
pub mod settings_view;
pub mod state;
pub mod toolbar;
pub mod view;

use crate::app::commands::AppCommand;
use crate::app::lifecycle::{
    confirmation_worthy, ExitSource, LifecycleCoordinator, LifecycleState,
};
use eframe::egui;
use state::AppState;
use std::sync::mpsc::{self, Receiver};

pub struct MultiMouseCanvasApp {
    state: AppState,
    command_rx: Receiver<AppCommand>,
    tray: Option<crate::tray::AppTray>,
    lifecycle: LifecycleCoordinator,
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
        let tray_result = crate::tray::AppTray::new(tx);
        let mut state = AppState::load();
        let (tray, tray_error) = match tray_result {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(e.to_string())),
        };
        state.tray_available = tray.is_some();
        state.tray_error = tray_error;
        for command in initial_commands {
            state.apply_command(command);
        }
        Self {
            state,
            command_rx: rx,
            tray,
            lifecycle: LifecycleCoordinator::default(),
        }
    }
}

impl eframe::App for MultiMouseCanvasApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(command) = self.command_rx.try_recv() {
            match command {
                AppCommand::Show if !self.lifecycle.is_preparing() => self.show_window(ctx),
                AppCommand::Exit => self.request_exit(ExitSource::CliOrIpc),
                AppCommand::ExitFromTray => self.request_exit(ExitSource::Tray),
                AppCommand::MinimizeToTray => self.minimize_to_tray(ctx),
                c if !self.lifecycle.is_preparing() => self.state.apply_command(c),
                _ => {
                    self.state.status_message =
                        Some("Command rejected: shutdown is in progress.".into())
                }
            }
        }
        self.state.flush_settings_save_if_due();
        if std::mem::take(&mut self.state.minimize_requested) {
            self.minimize_to_tray(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(ExitSource::WindowClose);
        }
        if self.lifecycle.take_checkpoint_request() {
            self.state.status_message = Some("Saving recovery and exiting…".into());
            self.state.prepare_shutdown_checkpoint();
            self.lifecycle.checkpoint_complete(Ok(()));
        }
        if self.lifecycle.poll_timeout(std::time::Instant::now()) {
            tracing::error!("shutdown-timeout: forcing shutdown after best-effort cleanup");
        }
        if matches!(
            self.lifecycle.state(),
            LifecycleState::ReadyToClose | LifecycleState::ForceExiting
        ) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(tray) = &mut self.tray {
            tray.update(&self.state, &self.lifecycle);
        }
        view::show(ctx, &mut self.state, &mut self.lifecycle);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.stop_sampler();
        self.state.save_settings_as_status();
    }
}

impl MultiMouseCanvasApp {
    fn request_exit(&mut self, source: ExitSource) {
        let worthy = confirmation_worthy(
            self.state.recording_status,
            !self.state.canvas.is_empty(),
            self.state.has_unexported_canvas,
        );
        self.lifecycle.exit_requested(source, worthy);
    }
    fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.state.ui_visible = true;
        self.state.status_message = Some("Window shown; visible update cadence restored.".into());
    }
    fn minimize_to_tray(&mut self, ctx: &egui::Context) {
        if !self.state.tray_available {
            self.state.status_message = Some(format!(
                "Cannot minimize to tray: {}",
                self.state
                    .tray_error
                    .as_deref()
                    .unwrap_or("tray unavailable")
            ));
            return;
        }
        self.state.ui_visible = false;
        self.state.status_message = Some("Running in tray; hidden update cadence enabled.".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }
}
