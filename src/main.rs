mod app;
mod canvas;
mod capture;
mod export;
mod platform;
mod session;
mod settings;

use tracing_subscriber::{fmt, EnvFilter};

fn main() -> eframe::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "MultiMouseCanvas",
        native_options,
        Box::new(|creation_context| Ok(Box::new(app::MultiMouseCanvasApp::new(creation_context)))),
    )
}
