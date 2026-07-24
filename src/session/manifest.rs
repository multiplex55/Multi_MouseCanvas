use crate::{
    app_colors::registry::ApplicationColorRegistry,
    canvas::{
        coordinates::SessionDesktopBounds,
        model::CanvasBackground,
        topology::{DisplayTopology, TopologyHistory},
    },
    session::{model::RecordingStatus, statistics::SessionStatistics},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const RECOVERY_SCHEMA_VERSION: u32 = 2;
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub schema_version: u32,
    pub session_id: String,
    pub started_at: SystemTime,
    pub saved_at: SystemTime,
    pub completed: bool,
    pub recording_status: RecordingStatus,
    pub session_bounds: SessionDesktopBounds,
    pub current_topology: DisplayTopology,
    pub topology_history: TopologyHistory,
    pub statistics: SessionStatistics,
    pub background: CanvasBackground,
    pub tile_size: u32,
    pub pixel_format: String,
    pub application_colors: ApplicationColorRegistry,
    #[serde(default)]
    pub tiles: Vec<String>,
}

/// Timestamp, process id, and a process-local monotonic sequence avoid collisions
/// without imposing a UUID dependency.
pub fn generate_session_id(now: SystemTime) -> String {
    let nanos = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "session-{nanos:032x}-{:08x}-{sequence:016x}",
        std::process::id()
    )
}

pub fn create_session_directory(
    root: &Path,
    now: SystemTime,
) -> std::io::Result<(String, std::path::PathBuf)> {
    fs::create_dir_all(root)?;
    loop {
        let id = generate_session_id(now);
        let path = root.join(&id);
        match fs::create_dir(&path) {
            Ok(()) => {
                fs::create_dir(path.join("tiles"))?;
                return Ok((id, path));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_collision_safe() {
        let now = UNIX_EPOCH;
        assert_ne!(generate_session_id(now), generate_session_id(now));
    }
}
