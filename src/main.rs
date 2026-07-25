#![cfg_attr(
    all(windows, not(debug_assertions), not(test)),
    windows_subsystem = "windows"
)]

mod app;
mod app_colors;
mod canvas;
mod capture;
mod display_profiles;
mod export;
mod ipc;
mod logging;
mod platform;
mod session;
mod settings;
mod tray;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn main() {
    let status = logging::initialize();
    let gui_exists = Arc::new(AtomicBool::new(false));
    install_panic_hook(gui_exists.clone(), status.expected_path.clone());
    let code = run(status, gui_exists);
    if code != 0 {
        std::process::exit(code);
    }
}

fn install_panic_hook(gui_exists: Arc<AtomicBool>, log_path: std::path::PathBuf) {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "fatal startup panic");
        if !gui_exists.load(Ordering::SeqCst) {
            platform::message_box::show_fatal(&fatal_text(
                "The application encountered a startup error.",
                &log_path,
                None,
            ));
        }
        original(info);
    }));
}

fn run(status: logging::LoggingStatus, gui_exists: Arc<AtomicBool>) -> i32 {
    tracing::debug!(
        writer_initialized = status.writer_initialized,
        subscriber_installed = status.subscriber_installed,
        "startup diagnostics status"
    );
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "application startup");
    platform::display::set_process_dpi_awareness();
    let commands = match app::commands::parse_cli_args(std::env::args().skip(1)) {
        Ok(commands) => commands,
        Err(app::commands::CliParseError::HelpRequested) => {
            tracing::info!(
                help = app::commands::cli_help_text(),
                "command-line help requested"
            );
            #[cfg(debug_assertions)]
            println!("{}", app::commands::cli_help_text());
            return 0;
        }
        Err(error) => {
            tracing::error!(?error, "invalid command-line arguments");
            #[cfg(debug_assertions)]
            eprintln!(
                "Invalid arguments: {error:?}\n{}",
                app::commands::cli_help_text()
            );
            return 2;
        }
    };
    let listener = match ipc::bind_listener() {
        Ok(listener) => {
            tracing::info!("primary instance selected");
            Some(listener)
        }
        Err(bind_error) => {
            tracing::info!(%bind_error, "primary endpoint unavailable; forwarding to existing instance");
            let forwarded = if commands.is_empty() {
                vec![app::commands::AppCommand::Show]
            } else {
                commands
            };
            for command in forwarded {
                if let Err(error) = ipc::forward_command(command) {
                    tracing::error!(?command, %error, "command forwarding failed");
                    platform::message_box::show_fatal(&fatal_text(
                        "Could not start or contact MultiMouseCanvas.",
                        &status.expected_path,
                        status.warning.as_deref(),
                    ));
                    return 3;
                }
                tracing::info!(?command, "forwarded semantic command");
            }
            tracing::info!("secondary instance forwarding complete");
            return 0;
        }
    };
    let warning = status.warning.clone();
    let result = eframe::run_native(
        "MultiMouseCanvas",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            gui_exists.store(true, Ordering::SeqCst);
            Ok(Box::new(
                app::MultiMouseCanvasApp::new_with_startup_warning(cc, listener, commands, warning),
            ))
        }),
    );
    match result {
        Ok(()) => {
            tracing::info!("application shutdown complete");
            0
        }
        Err(error) => {
            tracing::error!(%error, "display startup failed");
            platform::message_box::show_fatal(&fatal_text(
                "The application window could not be started.",
                &status.expected_path,
                status.warning.as_deref(),
            ));
            1
        }
    }
}

fn fatal_text(summary: &str, path: &Path, logging_warning: Option<&str>) -> String {
    let context = if logging_warning.is_some() {
        " Logging is unavailable."
    } else {
        ""
    };
    format!("{summary}{context}\nExpected log: {}", path.display())
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_windows_subsystem_attribute_is_present() {
        let source = include_str!("main.rs");
        assert!(source.contains("all(windows, not(debug_assertions), not(test))"));
        assert!(source.contains("windows_subsystem = \"windows\""));
    }
}
