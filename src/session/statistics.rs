use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStatistics {
    pub observed_samples: u64,
    pub accepted_movement_points: u64,
    pub simplified_discarded_points: u64,
    pub finalized_movement_chunks: u64,
    pub finalized_dwells: u64,
    pub active_tile_count: usize,
    pub dirty_tile_count: usize,
    pub actively_recorded_duration: Duration,
    pub samples_recorded: u64,
    pub movements_recorded: u64,
    pub dwell_events: u64,
    pub session_duration: Duration,
    pub total_cursor_distance: f32,
    pub finalized_dwell_count: u64,
    pub current_dwell_duration: Duration,
    pub longest_dwell: Duration,
    pub movement_segment_count: u64,
}

impl SessionStatistics {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
