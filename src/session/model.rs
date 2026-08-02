use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingStatus {
    /// No active sampler and no completed canvas retained by the engine.
    Stopped,
    Recording,
    Paused,
    /// Recording is finalized; its canvas remains available for export or clear.
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct SessionTiming {
    pub started_at: Option<SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_status_names_still_deserialize() {
        assert_eq!(
            serde_json::from_str::<RecordingStatus>("\"Stopped\"").unwrap(),
            RecordingStatus::Stopped
        );
        assert_eq!(
            serde_json::from_str::<RecordingStatus>("\"Recording\"").unwrap(),
            RecordingStatus::Recording
        );
        assert_eq!(
            serde_json::from_str::<RecordingStatus>("\"Paused\"").unwrap(),
            RecordingStatus::Paused
        );
    }
}
