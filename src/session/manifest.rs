use crate::{
    app_colors::registry::ApplicationColorRegistry,
    canvas::{
        coordinates::SessionDesktopBounds,
        model::{CanvasBackground, CanvasModel},
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
    #[serde(default = "empty_topology")]
    pub detected_topology: DisplayTopology,
    #[serde(alias = "current_topology", default = "empty_topology")]
    pub effective_topology: DisplayTopology,
    #[serde(default = "empty_topology")]
    pub session_topology: DisplayTopology,
    #[serde(default)]
    pub topology_history: TopologyHistory,
    pub statistics: SessionStatistics,
    pub background: CanvasBackground,
    pub tile_size: u32,
    pub pixel_format: String,
    pub application_colors: ApplicationColorRegistry,
    #[serde(default)]
    pub profile_snapshot: Option<crate::display_profiles::DisplayProfileSnapshot>,
    #[serde(default)]
    pub tiles: Vec<String>,
}

impl SessionManifest {
    /// The single construction path for recovery checkpoints. Keeping derived
    /// canvas fields here prevents normal autosaves, imports, and tests from
    /// silently producing different manifest shapes.
    #[allow(clippy::too_many_arguments)]
    pub fn checkpoint(
        session_id: String,
        started_at: SystemTime,
        saved_at: SystemTime,
        completed: bool,
        recording_status: RecordingStatus,
        canvas: &CanvasModel,
        statistics: SessionStatistics,
        application_colors: ApplicationColorRegistry,
        profile_snapshot: Option<crate::display_profiles::DisplayProfileSnapshot>,
    ) -> Self {
        Self {
            schema_version: RECOVERY_SCHEMA_VERSION,
            session_id,
            started_at,
            saved_at,
            completed,
            recording_status,
            session_bounds: canvas.session_desktop_bounds,
            detected_topology: canvas.detected_topology.clone(),
            effective_topology: canvas.effective_topology.clone(),
            session_topology: canvas
                .topology_history
                .entries
                .first()
                .cloned()
                .unwrap_or_else(|| canvas.effective_topology.clone()),
            topology_history: canvas.topology_history.clone(),
            statistics,
            background: canvas.background.clone(),
            tile_size: canvas.sparse_tiles.tile_size,
            pixel_format: "RGBA8".into(),
            application_colors,
            profile_snapshot,
            tiles: canvas
                .sparse_tiles
                .tiles
                .keys()
                .map(|c| crate::session::recovery::tile_filename(*c))
                .collect(),
        }
    }
}

fn empty_topology() -> DisplayTopology {
    DisplayTopology::new(vec![])
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
