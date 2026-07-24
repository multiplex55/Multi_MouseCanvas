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
    }

    impl AppTray {
        pub fn new(tx: Sender<AppCommand>) -> Option<Self> {
            let menu = Menu::new();
            let items = [
                ("show", "Show MultiMouseCanvas", AppCommand::Show),
                ("start", "Start recording", AppCommand::StartRecording),
                (
                    "toggle_pause_resume",
                    "Pause/resume",
                    AppCommand::TogglePauseResume,
                ),
                ("finish", "Finish session", AppCommand::FinishSession),
                (
                    "export",
                    "Export current canvas",
                    AppCommand::ExportCurrentCanvas,
                ),
                ("exit", "Exit", AppCommand::Exit),
            ];
            for (id, title, _) in items {
                let _ = menu.append(&MenuItem::with_id(id, title, true, None));
            }
            let icon = Icon::from_rgba(vec![0, 120, 215, 255; 16 * 16], 16, 16).ok()?;
            let tray = TrayIconBuilder::new()
                .with_tooltip("MultiMouseCanvas")
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .build()
                .ok()?;
            MenuEvent::set_event_handler(Some(move |event| {
                let command = match event.id().as_ref() {
                    "show" => AppCommand::Show,
                    "start" => AppCommand::StartRecording,
                    "toggle_pause_resume" => AppCommand::TogglePauseResume,
                    "finish" => AppCommand::FinishSession,
                    "export" => AppCommand::ExportCurrentCanvas,
                    "exit" => AppCommand::Exit,
                    _ => return,
                };
                let _ = tx.send(command);
            }));
            Some(Self { _icon: tray })
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;
    pub struct AppTray;
    impl AppTray {
        pub fn new(_tx: Sender<AppCommand>) -> Option<Self> {
            None
        }
    }
}
pub use imp::AppTray;
