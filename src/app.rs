pub mod application_editor;
pub mod commands;
pub mod dialogs;
pub mod display_profile_editor;
pub mod engine_bridge;
pub mod lifecycle;
pub mod monitor_identification;
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
    engine: engine_bridge::EngineBridge,
    monitor_identification: monitor_identification::MonitorIdentificationController,
}

impl MultiMouseCanvasApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        listener: Option<std::net::TcpListener>,
        initial_commands: Vec<AppCommand>,
    ) -> Self {
        Self::new_with_startup_warning(_cc, listener, initial_commands, None)
    }

    pub fn new_with_startup_warning(
        _cc: &eframe::CreationContext<'_>,
        listener: Option<std::net::TcpListener>,
        initial_commands: Vec<AppCommand>,
        startup_warning: Option<String>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        if let Some(listener) = listener {
            crate::ipc::serve(listener, tx.clone());
        }
        let tray_result = crate::tray::AppTray::new(tx);
        let mut state = AppState::load();
        let mut engine = engine_bridge::EngineBridge::spawn(state.settings.clone());
        engine.submit(
            &mut state,
            crate::session::events::EngineCommand::RequestSnapshot,
        );
        let (tray, tray_error) = match tray_result {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(e.to_string())),
        };
        state.tray_available = tray.is_some();
        state.tray_error = tray_error;
        if state.tray_available {
            tracing::info!("system tray available");
        } else {
            tracing::warn!(
                reason = state.tray_error.as_deref().unwrap_or("unknown"),
                "system tray unavailable"
            );
        }
        for command in initial_commands {
            state.apply_command(command);
        }
        if let Some(warning) = startup_warning {
            state.status_message = Some(format!("Diagnostics warning: {warning}"));
        }
        Self {
            state,
            command_rx: rx,
            tray,
            lifecycle: LifecycleCoordinator::default(),
            engine,
            monitor_identification: Default::default(),
        }
    }
}

impl eframe::App for MultiMouseCanvasApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.state.identify_monitors_requested) {
            self.monitor_identification.show();
        }
        if self
            .monitor_identification
            .poll(&mut self.state.monitor_identification_status)
        {
            ctx.request_repaint();
        }
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
        if std::mem::take(&mut self.state.retry_engine_requested) {
            self.engine.retry(&mut self.state);
        }
        self.process_pending_start();
        self.engine.flush_commands(&mut self.state);
        if self.engine.drain(&mut self.state) {
            ctx.request_repaint();
        }
        if std::mem::take(&mut self.state.minimize_requested) {
            self.minimize_to_tray(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(ExitSource::WindowClose);
        }
        if self.lifecycle.take_checkpoint_request() {
            self.monitor_identification.close();
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
        if self.monitor_identification.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.monitor_identification.shutdown();
        self.engine.shutdown();
        self.state.save_settings_as_status();
        tracing::info!("GUI shutdown cleanup complete");
    }
}

impl MultiMouseCanvasApp {
    fn request_exit(&mut self, source: ExitSource) {
        let worthy = confirmation_worthy(
            self.state.recording_status,
            !self.state.preview.is_empty(),
            self.state.has_unexported_canvas,
        );
        self.lifecycle.exit_requested(source, worthy);
    }
    fn show_window(&mut self, ctx: &egui::Context) {
        tracing::info!("application window shown");
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.state.ui_visible = true;
        self.engine.submit(
            &mut self.state,
            crate::session::events::EngineCommand::SetUiVisibility(true),
        );
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
        self.engine.submit(
            &mut self.state,
            crate::session::events::EngineCommand::SetUiVisibility(false),
        );
        tracing::info!("application window hidden to tray");
        self.state.status_message = Some("Running in tray; hidden update cadence enabled.".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }
}

impl MultiMouseCanvasApp {
    fn process_pending_start(&mut self) {
        if !std::mem::take(&mut self.state.automatic_start_pending) {
            return;
        }
        let Ok(detected) = crate::platform::display::current_topology() else {
            self.state.status_message =
                Some("Display detection failed; recording was not started.".into());
            return;
        };
        if let Some(profile) = self
            .state
            .display_profiles
            .exact_match(&detected)
            .and_then(|p| p.snapshot(&detected))
        {
            let resolved = crate::session::events::ResolvedDisplayProfile {
                settings: std::sync::Arc::new(self.state.settings.clone()),
                detected_topology: detected,
                effective_topology: profile.effective_topology.clone(),
                profile: std::sync::Arc::new(profile.clone()),
            };
            if self.engine.submit(
                &mut self.state,
                crate::session::events::EngineCommand::Start(resolved),
            ) {
                self.state.active_display_profile = Some(std::sync::Arc::new(profile))
            }
        } else {
            self.state.monitor_selection = Some(
                crate::app::monitor_selection::MonitorSelectionState::new(detected),
            );
        }
    }
}
