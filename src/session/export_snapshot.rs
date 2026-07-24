use crate::canvas::coordinates::TileCoordinate;
use crate::{
    canvas::{
        coordinates::VirtualDesktopBounds,
        model::{CanvasBackground, DwellShape, MovementPath},
        topology::DisplayTopology,
    },
    session::statistics::SessionStatistics,
};
use std::{collections::HashMap, sync::Arc, time::SystemTime};

/// Point-in-time, immutable input to an export worker.
#[derive(Debug, Clone)]
pub struct ExportSnapshot {
    pub sequence: u64,
    pub generation: u64,
    pub bounds: VirtualDesktopBounds,
    pub tile_size: u32,
    pub tiles: HashMap<TileCoordinate, Arc<[u8]>>,
    pub active_path: Option<MovementPath>,
    pub active_dwell: Option<DwellShape>,
    pub background: CanvasBackground,
    pub topology: DisplayTopology,
    pub statistics: SessionStatistics,
    pub captured_at: SystemTime,
}
