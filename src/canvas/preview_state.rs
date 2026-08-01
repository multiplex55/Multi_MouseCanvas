use crate::{
    canvas::{model::CanvasModel, tiles::Tile},
    session::{snapshot::SessionSnapshot, statistics::SessionStatistics},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewApplyError {
    OlderGeneration,
    StaleSequence,
    DeltaGeneration,
    InvalidPixels,
}

/// Read-only-from-the-UI mirror of engine output. Only `apply_snapshot` may
/// change its canvas; it is never used for recovery, capture, or export.
#[derive(Clone)]
pub struct PreviewState {
    canvas: CanvasModel,
    revisions: HashMap<crate::canvas::coordinates::TileCoordinate, u64>,
    pub generation: u64,
    pub latest_sequence: u64,
    pub statistics: SessionStatistics,
    pub texture_epoch: u64,
}
impl Default for PreviewState {
    fn default() -> Self {
        Self {
            canvas: Default::default(),
            revisions: Default::default(),
            generation: 0,
            latest_sequence: 0,
            statistics: Default::default(),
            texture_epoch: 0,
        }
    }
}
impl PreviewState {
    pub fn canvas(&self) -> &CanvasModel {
        &self.canvas
    }
    pub fn is_empty(&self) -> bool {
        self.canvas.is_empty()
    }
    pub fn apply_snapshot(&mut self, s: &SessionSnapshot) -> Result<(), PreviewApplyError> {
        if s.generation < self.generation {
            return Err(PreviewApplyError::OlderGeneration);
        }
        if s.generation == self.generation && s.sequence < self.latest_sequence {
            return Err(PreviewApplyError::StaleSequence);
        }
        if s.tile_deltas.iter().any(|d| d.generation != s.generation) {
            return Err(PreviewApplyError::DeltaGeneration);
        }
        if s.tile_deltas
            .iter()
            .any(|d| !d.removed && d.rgba.len() != (d.width * d.height * 4) as usize)
        {
            return Err(PreviewApplyError::InvalidPixels);
        }
        if s.generation > self.generation {
            self.canvas.clear();
            self.revisions.clear();
            self.texture_epoch += 1;
            self.generation = s.generation;
            self.canvas.tile_generation = s.generation;
            self.latest_sequence = 0;
        }
        let incoming: HashSet<_> = s
            .tile_deltas
            .iter()
            .filter(|d| !d.removed)
            .map(|d| d.coordinate)
            .collect();
        if s.full_tile_snapshot {
            self.canvas
                .sparse_tiles
                .tiles
                .retain(|c, _| incoming.contains(c));
            self.revisions.retain(|c, _| incoming.contains(c));
            self.texture_epoch += 1;
        }
        for d in &s.tile_deltas {
            if d.removed {
                self.canvas.sparse_tiles.tiles.remove(&d.coordinate);
                self.revisions.remove(&d.coordinate);
                self.texture_epoch += 1;
                continue;
            }
            if self
                .revisions
                .get(&d.coordinate)
                .is_some_and(|r| *r >= d.revision)
            {
                continue;
            }
            self.canvas.sparse_tiles.tile_size = d.width;
            self.canvas.sparse_tiles.tiles.insert(
                d.coordinate,
                Tile {
                    pixels: d.rgba.to_vec(),
                    preview_dirty: true,
                    recovery_dirty: false,
                    revision: d.revision,
                    contains_artwork: true,
                },
            );
            self.revisions.insert(d.coordinate, d.revision);
        }
        self.canvas.session_desktop_bounds = s.session_bounds;
        self.canvas.current_topology = s.effective_topology.clone();
        self.canvas.active_movement_overlay = s.active_path_overlay.clone();
        self.canvas.active_dwell_overlay = s.active_dwell_overlay.clone();
        self.statistics = s.statistics.clone();
        self.latest_sequence = s.sequence;
        Ok(())
    }
}
