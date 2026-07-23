use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStatistics {
    pub samples_recorded: u64,
    pub movements_recorded: u64,
    pub dwell_events: u64,
}

impl SessionStatistics {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
