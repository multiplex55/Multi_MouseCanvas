use super::{
    coordinates::{CanvasPoint, VirtualDesktopBounds},
    tiles::SparseTileStore,
    topology::{DisplayTopology, TopologyHistory},
};
use crate::{
    capture::foreground::ApplicationIdentity,
    settings::model::{DwellRenderMode, DwellShapeKind, RgbaColor},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_CANVAS_WIDTH: f32 = 1920.0;
pub const DEFAULT_CANVAS_HEIGHT: f32 = 1080.0;
pub const DEFAULT_POINT_MERGE_DISTANCE: f32 = 1.5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasBackground {
    pub color: RgbaColor,
    /// Transparent mode affects preview/export compositing decisions, not the
    /// logical background color retained for exports.
    pub transparent: bool,
}

impl Default for CanvasBackground {
    fn default() -> Self {
        Self {
            color: RgbaColor::new(24, 24, 24, 255),
            transparent: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovementPath {
    pub points: Vec<CanvasPoint>,
    pub color: RgbaColor,
    pub width: f32,
    pub finalized: bool,
    pub application: ApplicationIdentity,
}

impl MovementPath {
    pub fn new(color: RgbaColor, width: f32, finalized: bool) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
            finalized,
            application: ApplicationIdentity::default(),
        }
    }

    pub fn push_simplified(&mut self, point: CanvasPoint, merge_distance: f32) {
        if self
            .points
            .last()
            .is_none_or(|last| point_distance(*last, point) >= merge_distance)
        {
            self.points.push(point);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DwellShape {
    pub center: CanvasPoint,
    pub duration: Duration,
    pub size: f32,
    pub color: RgbaColor,
    pub shape_kind: DwellShapeKind,
    pub fill_opacity: f32,
    pub outline_width: f32,
    pub render_mode: DwellRenderMode,
    pub finalized: bool,
    pub application: ApplicationIdentity,
}

impl DwellShape {
    #[allow(clippy::too_many_arguments)]
    pub fn from_duration(
        center: CanvasPoint,
        duration: Duration,
        color: RgbaColor,
        shape_kind: DwellShapeKind,
        min_size: f32,
        max_size: f32,
        growth_rate: f32,
        fill_opacity: f32,
        outline_width: f32,
        render_mode: DwellRenderMode,
        finalized: bool,
    ) -> Self {
        let size = dwell_size(duration, min_size, max_size, growth_rate);
        Self {
            center,
            duration,
            size,
            color,
            shape_kind,
            fill_opacity,
            outline_width,
            render_mode,
            finalized,
            application: ApplicationIdentity::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasModel {
    pub background: CanvasBackground,
    pub sparse_tiles: SparseTileStore,
    pub session_desktop_bounds: VirtualDesktopBounds,
    pub current_topology: DisplayTopology,
    pub topology_history: TopologyHistory,
    pub active_movement_overlay: Option<MovementPath>,
    pub active_dwell_overlay: Option<DwellShape>,
    pub point_merge_distance: f32,
    pub committed_movement_count: usize,
    pub committed_dwell_count: usize,
    pub tile_generation: u64,
    #[serde(skip)]
    pub dimensions: (f32, f32),
}

impl Default for CanvasModel {
    fn default() -> Self {
        let current_topology = DisplayTopology::default();
        let session_desktop_bounds = current_topology.bounds().unwrap_or_else(|| {
            VirtualDesktopBounds::new(0.0, 0.0, DEFAULT_CANVAS_WIDTH, DEFAULT_CANVAS_HEIGHT)
        });
        Self {
            background: CanvasBackground::default(),
            sparse_tiles: SparseTileStore::default(),
            session_desktop_bounds,
            current_topology,
            topology_history: TopologyHistory::default(),
            active_movement_overlay: None,
            active_dwell_overlay: None,
            point_merge_distance: DEFAULT_POINT_MERGE_DISTANCE,
            committed_movement_count: 0,
            committed_dwell_count: 0,
            tile_generation: 0,
            dimensions: (
                session_desktop_bounds.width(),
                session_desktop_bounds.height(),
            ),
        }
    }
}

impl CanvasModel {
    pub fn clear(&mut self) {
        self.sparse_tiles.tiles.clear();
        self.active_movement_overlay = None;
        self.active_dwell_overlay = None;
        self.committed_movement_count = 0;
        self.committed_dwell_count = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.sparse_tiles.tiles.is_empty()
            && self.active_movement_overlay.is_none()
            && self.active_dwell_overlay.is_none()
    }
    pub fn canvas_dimensions(&self) -> (f32, f32) {
        (
            self.session_desktop_bounds.width(),
            self.session_desktop_bounds.height(),
        )
    }

    pub fn refresh_dimensions(&mut self) {
        self.dimensions = self.canvas_dimensions();
    }
}

pub fn dwell_size(duration: Duration, min_size: f32, max_size: f32, growth_rate: f32) -> f32 {
    (min_size + duration.as_secs_f32() * growth_rate).clamp(min_size, max_size.max(min_size))
}

pub fn shape_geometry(kind: DwellShapeKind, center: CanvasPoint, size: f32) -> Vec<CanvasPoint> {
    let r = size / 2.0;
    match kind {
        DwellShapeKind::Circle => vec![
            CanvasPoint {
                x: center.x - r,
                y: center.y - r,
            },
            CanvasPoint {
                x: center.x + r,
                y: center.y + r,
            },
        ],
        DwellShapeKind::Triangle => vec![
            CanvasPoint {
                x: center.x,
                y: center.y - r,
            },
            CanvasPoint {
                x: center.x - r,
                y: center.y + r,
            },
            CanvasPoint {
                x: center.x + r,
                y: center.y + r,
            },
        ],
        DwellShapeKind::Square => vec![
            CanvasPoint {
                x: center.x - r,
                y: center.y - r,
            },
            CanvasPoint {
                x: center.x + r,
                y: center.y - r,
            },
            CanvasPoint {
                x: center.x + r,
                y: center.y + r,
            },
            CanvasPoint {
                x: center.x - r,
                y: center.y + r,
            },
        ],
    }
}

pub fn point_distance(a: CanvasPoint, b: CanvasPoint) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_simplification_merges_near_identical_points() {
        let mut path = MovementPath::new(RgbaColor::new(1, 2, 3, 255), 2.0, false);
        path.push_simplified(CanvasPoint { x: 0.0, y: 0.0 }, 2.0);
        path.push_simplified(CanvasPoint { x: 1.0, y: 1.0 }, 2.0);
        path.push_simplified(CanvasPoint { x: 3.0, y: 0.0 }, 2.0);
        assert_eq!(path.points.len(), 2);
    }

    #[test]
    fn dwell_shape_size_respects_min_max_growth() {
        assert_eq!(dwell_size(Duration::ZERO, 10.0, 30.0, 5.0), 10.0);
        assert_eq!(dwell_size(Duration::from_secs(10), 10.0, 30.0, 5.0), 30.0);
        assert_eq!(dwell_size(Duration::from_secs(2), 10.0, 30.0, 5.0), 20.0);
    }

    #[test]
    fn shape_geometry_has_expected_counts_and_bounds() {
        let center = CanvasPoint { x: 10.0, y: 10.0 };
        assert_eq!(shape_geometry(DwellShapeKind::Circle, center, 8.0).len(), 2);
        assert_eq!(
            shape_geometry(DwellShapeKind::Triangle, center, 8.0).len(),
            3
        );
        let square = shape_geometry(DwellShapeKind::Square, center, 8.0);
        assert_eq!(square.len(), 4);
        assert!(square
            .iter()
            .all(|p| (6.0..=14.0).contains(&p.x) && (6.0..=14.0).contains(&p.y)));
    }

    #[test]
    fn transparent_background_does_not_alter_logical_export_background() {
        let mut canvas = CanvasModel::default();
        canvas.background.color = RgbaColor::new(7, 8, 9, 255);
        canvas.background.transparent = true;
        assert_eq!(canvas.background.color, RgbaColor::new(7, 8, 9, 255));
    }
}
