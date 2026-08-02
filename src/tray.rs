use crate::app::commands::AppCommand;
use std::sync::mpsc::Sender;

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        Icon, TrayIcon, TrayIconBuilder,
    };

    pub struct AppTray {
        _icon: TrayIcon,
        show: MenuItem,
        start: MenuItem,
        pause: MenuItem,
        finish: MenuItem,
        export: MenuItem,
        exit: MenuItem,
    }

    impl AppTray {
        pub fn new(tx: Sender<AppCommand>) -> Result<Self, TrayInitError> {
            let menu = Menu::new();
            let show = MenuItem::with_id("show", "Show MultiMouseCanvas", true, None);
            let start = MenuItem::with_id("start", "Start recording", true, None);
            let pause = MenuItem::with_id("toggle_pause_resume", "Pause", false, None);
            let finish = MenuItem::with_id("finish", "Finish session", false, None);
            let export = MenuItem::with_id("export", "Export current canvas", false, None);
            let exit = MenuItem::with_id("exit", "Exit", true, None);
            for item in [&show, &start, &pause, &finish, &export, &exit] {
                menu.append(item)
                    .map_err(|e| TrayInitError(format!("create tray menu: {e}")))?;
            }
            let rgba = [0_u8, 120, 215, 255].repeat(16 * 16);
            let icon = Icon::from_rgba(rgba, 16, 16)
                .map_err(|e| TrayInitError(format!("create tray icon: {e}")))?;
            let tray = TrayIconBuilder::new()
                .with_tooltip("MultiMouseCanvas")
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .build()
                .map_err(|e| TrayInitError(format!("initialize system tray: {e}")))?;
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let command = match event.id().as_ref() {
                    "show" => AppCommand::Show,
                    "start" => AppCommand::StartRecording,
                    "toggle_pause_resume" => AppCommand::TogglePauseResume,
                    "finish" => AppCommand::FinishSession,
                    "export" => AppCommand::ExportCurrentCanvas,
                    "exit" => AppCommand::ExitFromTray,
                    _ => return,
                };
                let _ = tx.send(command);
            }));
            Ok(Self {
                _icon: tray,
                show,
                start,
                pause,
                finish,
                export,
                exit,
            })
        }
        pub fn update(
            &mut self,
            state: &crate::app::state::AppState,
            lifecycle: &crate::app::lifecycle::LifecycleCoordinator,
        ) {
            let policy = crate::app::action_policy::action_policy(
                state.recording_status,
                state.pending_transition.is_some(),
                state.export_busy,
                !state.preview.is_empty(),
                state.capture_health.as_ref().is_none_or(|h| {
                    h.engine == crate::session::snapshot::EngineConnectionState::Connected
                }),
                lifecycle.is_preparing(),
            );
            self.show.set_enabled(!lifecycle.is_preparing());
            self.start.set_enabled(policy.start.enabled);
            self.pause.set_text(policy.pause_resume.label);
            self.pause.set_enabled(policy.pause_resume.enabled);
            self.finish.set_enabled(policy.finish.enabled);
            self.export.set_enabled(policy.export.enabled);
            self.exit.set_enabled(!lifecycle.is_preparing());
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;
    pub struct AppTray;
    impl AppTray {
        pub fn new(_tx: Sender<AppCommand>) -> Result<Self, TrayInitError> {
            Err(TrayInitError(
                "system tray is only available on Windows".into(),
            ))
        }
        pub fn update(
            &mut self,
            _state: &crate::app::state::AppState,
            _lifecycle: &crate::app::lifecycle::LifecycleCoordinator,
        ) {
        }
    }
}
#[derive(Debug, Clone)]
pub struct TrayInitError(pub String);
impl std::fmt::Display for TrayInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TrayInitError {}
pub use imp::AppTray;
