mod app;
mod app_colors;
mod canvas;
mod capture;
mod export;
mod ipc;
mod platform;
mod session;
mod settings;
mod tray;

use tracing_subscriber::{fmt, EnvFilter};

fn main() -> eframe::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let commands = match app::commands::parse_cli_args(std::env::args().skip(1)) {
        Ok(commands) => commands,
        Err(error) => {
            eprintln!("Invalid arguments: {error:?}");
            return Ok(());
        }
    };
    let listener = match ipc::bind_listener() {
        Ok(listener) => Some(listener),
        Err(_) if !commands.is_empty() => {
            for command in commands {
                let _ = ipc::forward_command(command);
            }
            return Ok(());
        }
        Err(_) => {
            eprintln!("MultiMouseCanvas is already running.");
            return Ok(());
        }
    };
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "MultiMouseCanvas",
        native_options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::MultiMouseCanvasApp::new(
                creation_context,
                listener,
                commands,
            )))
        }),
    )
}
