use crate::{
    canvas::{
        coordinates::{TileCoordinate, VirtualDesktopBounds},
        model::{DwellShape, MovementPath},
        topology::DisplayTopology,
    },
    session::{error::EngineError, model::RecordingStatus, statistics::SessionStatistics},
    settings::model::AppSettings,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct TileDelta {
    pub coordinate: TileCoordinate,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub removed: bool,
    pub generation: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineActivity {
    pub export_in_progress: bool,
    pub recovery_in_progress: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    pub recording_status: RecordingStatus,
    pub session_id: Option<String>,
    pub detected_topology: DisplayTopology,
    pub effective_topology: DisplayTopology,
    pub session_bounds: VirtualDesktopBounds,
    pub profile: Option<Arc<AppSettings>>,
    pub tile_deltas: Vec<TileDelta>,
    pub full_tile_snapshot: bool,
    pub active_path_overlay: Option<MovementPath>,
    pub active_dwell_overlay: Option<DwellShape>,
    /// Retained as a cheap compatibility/indexing view for diagnostics.
    pub changed_tile_revisions: HashMap<TileCoordinate, u64>,
    pub current_topology: DisplayTopology,
    pub session_topology: DisplayTopology,
    pub statistics: SessionStatistics,
    pub sampler_observed: u64,
    pub classifier_delivered: u64,
    pub samples_coalesced: u64,
    pub activity: EngineActivity,
    pub status_messages: Vec<String>,
    pub errors: Vec<EngineError>,
    pub sequence: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotKey {
    status: RecordingStatus,
    generation: u64,
    revisions: Vec<(TileCoordinate, u64, bool)>,
    overlay_key: (usize, bool),
    stats: (u64, u64, u64),
    activity: EngineActivity,
    message_count: usize,
    error_count: usize,
}

#[derive(Debug, Default)]
pub struct SnapshotDeduper {
    last: Option<SnapshotKey>,
}
impl SnapshotDeduper {
    pub fn should_send(&mut self, next: &SessionSnapshot) -> bool {
        let mut revisions: Vec<_> = next
            .tile_deltas
            .iter()
            .map(|d| (d.coordinate, d.revision, d.removed))
            .collect();
        revisions.sort_by_key(|v| (v.0.x, v.0.y));
        let key = SnapshotKey {
            status: next.recording_status,
            generation: next.generation,
            revisions,
            overlay_key: (
                next.active_path_overlay
                    .as_ref()
                    .map_or(0, |p| p.points.len()),
                next.active_dwell_overlay.is_some(),
            ),
            stats: (
                next.sampler_observed,
                next.classifier_delivered,
                next.samples_coalesced,
            ),
            activity: next.activity.clone(),
            message_count: next.status_messages.len(),
            error_count: next.errors.len(),
        };
        if self.last.as_ref() == Some(&key) {
            return false;
        }
        self.last = Some(key);
        true
    }
    pub fn clear(&mut self) {
        self.last = None;
    }
}
