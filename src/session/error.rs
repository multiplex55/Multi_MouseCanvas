use thiserror::Error;

/// Errors crossing the engine/UI boundary. Messages intentionally contain no
/// cursor positions, window titles, or URLs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("cursor sampler failed: {0}")]
    Sampler(String),
    #[error("display enumeration failed: {0}")]
    DisplayEnumeration(String),
    #[error("foreground application metadata is temporarily unavailable: {0}")]
    ForegroundDegradation(String),
    #[error("recovery checkpoint failed: {0}")]
    RecoveryWrite(String),
    #[error("export failed: {0}")]
    Export(String),
    #[error("recording engine failed: {0}")]
    Internal(String),
}

impl EngineError {
    pub fn requires_recording_stop(&self) -> bool {
        matches!(self, Self::Sampler(_) | Self::Internal(_))
    }
}
