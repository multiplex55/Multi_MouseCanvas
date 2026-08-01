//! MultiMouseCanvas application library.
//!
//! Keeping the application domains in a library target makes their internal
//! contracts available to platform integrations and integration tests. The
//! executable remains a thin startup/diagnostics entry point, and Rust can now
//! report genuinely unreachable private code instead of warning about every
//! public binary-crate API.

pub mod app;
pub mod app_colors;
pub mod canvas;
pub mod capture;
pub mod display_profiles;
pub mod export;
pub mod ipc;
pub mod logging;
pub mod platform;
pub mod session;
pub mod settings;
pub mod tray;
