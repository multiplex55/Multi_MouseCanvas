use crate::{
    canvas::{model::CanvasModel, tiles::Tile},
    session::{snapshot::SessionSnapshot, statistics::SessionStatistics},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewViewMode {
    #[default]
    FitAll,
    ActualSize,
}

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
    /// UI-only state: never enters an engine snapshot or export model.
    pub view_mode: PreviewViewMode,
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
            view_mode: PreviewViewMode::FitAll,
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
    /// Invalidates every cache whose identity is scoped to an engine generation.
    pub fn invalidate_generation(&mut self, generation: u64) {
        if generation <= self.generation {
            return;
        }
        self.canvas.clear();
        self.revisions.clear();
        self.texture_epoch = self.texture_epoch.saturating_add(1);
        self.generation = generation;
        self.canvas.tile_generation = generation;
        self.latest_sequence = 0;
    }

    /// A correlated full-state response is a protocol barrier, not a delta.
    pub fn prepare_authoritative_full_state(&mut self, generation: u64) {
        self.canvas.clear();
        self.revisions.clear();
        self.texture_epoch = self.texture_epoch.saturating_add(1);
        self.generation = generation;
        self.canvas.tile_generation = generation;
        self.latest_sequence = 0;
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
            self.invalidate_generation(s.generation);
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
        self.canvas.detected_topology = s.detected_topology.clone();
        self.canvas.effective_topology = s.effective_topology.clone();
        self.canvas.topology_history = s.topology_history.clone();
        self.canvas.active_movement_overlay = s.active_path_overlay.clone();
        self.canvas.active_dwell_overlay = s.active_dwell_overlay.clone();
        self.statistics = s.statistics.clone();
        self.latest_sequence = s.sequence;
        Ok(())
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;

    #[test]
    fn generation_change_invalidates_canvas_revisions_and_textures() {
        let mut preview = PreviewState::default();
        preview.revisions.insert(
            crate::canvas::coordinates::TileCoordinate { x: 4, y: -2 },
            9,
        );
        let epoch = preview.texture_epoch;

        preview.invalidate_generation(7);

        assert_eq!(preview.generation, 7);
        assert_eq!(preview.latest_sequence, 0);
        assert!(preview.revisions.is_empty());
        assert!(preview.canvas.is_empty());
        assert!(preview.texture_epoch > epoch);
    }
}
