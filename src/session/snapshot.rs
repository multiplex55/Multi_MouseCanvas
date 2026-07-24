use crate::{
    canvas::{
        coordinates::TileCoordinate,
        model::{DwellShape, MovementPath},
        topology::DisplayTopology,
    },
    session::{model::RecordingStatus, statistics::SessionStatistics},
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    pub recording_status: RecordingStatus,
    pub active_path_overlay: Option<MovementPath>,
    pub active_dwell_overlay: Option<DwellShape>,
    pub changed_tile_revisions: HashMap<TileCoordinate, u64>,
    pub current_topology: DisplayTopology,
    pub session_topology: DisplayTopology,
    pub statistics: SessionStatistics,
    pub status_messages: Vec<String>,
    pub errors: Vec<String>,
    pub sequence: u64,
}

#[derive(Debug, Default)]
pub struct SnapshotDeduper {
    last: Option<SessionSnapshot>,
}
impl SnapshotDeduper {
    pub fn should_send(&mut self, next: &SessionSnapshot) -> bool {
        if self.last.as_ref() == Some(next) {
            return false;
        }
        self.last = Some(next.clone());
        true
    }
    pub fn clear(&mut self) {
        self.last = None;
    }
}
