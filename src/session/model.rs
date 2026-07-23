use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingStatus {
    Stopped,
    Recording,
    Paused,
}

#[derive(Debug, Clone, Default)]
pub struct SessionTiming {
    pub started_at: Option<SystemTime>,
}
