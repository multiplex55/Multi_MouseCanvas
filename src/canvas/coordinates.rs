use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct DesktopPoint {
    pub x: f32,
    pub y: f32,
}
pub type CanvasPoint = DesktopPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoordinate {
    pub x: i32,
    pub y: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileLocalPixel {
    pub x: u16,
    pub y: u16,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviewPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesktopRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}
pub type MonitorBounds = DesktopRect;
pub type SessionDesktopBounds = DesktopRect;
pub type VirtualDesktopBounds = DesktopRect;
pub type DisplayBounds = DesktopRect;

impl DesktopPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
impl DesktopRect {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }
    pub fn from_displays(displays: &[DisplayBounds]) -> Option<Self> {
        Self::union_all(displays)
    }
    pub fn union_all(rects: &[Self]) -> Option<Self> {
        let first = *rects.first()?;
        Some(rects.iter().skip(1).fold(first, |acc, r| acc.union(r)))
    }
    pub fn union(self, other: &Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
    pub fn width(&self) -> f32 {
        (self.max_x - self.min_x).max(1.0)
    }
    pub fn height(&self) -> f32 {
        (self.max_y - self.min_y).max(1.0)
    }
    pub fn contains(&self, p: DesktopPoint) -> bool {
        p.x >= self.min_x && p.x < self.max_x && p.y >= self.min_y && p.y < self.max_y
    }
    pub fn to_preview(&self, p: DesktopPoint) -> PreviewPoint {
        PreviewPoint {
            x: p.x - self.min_x,
            y: p.y - self.min_y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_coordinates_are_preserved_not_percent_normalized() {
        let b = DesktopRect::new(-1920.0, -200.0, 1920.0, 1080.0);
        assert_eq!(
            b.to_preview(DesktopPoint::new(-1920.0, -200.0)),
            PreviewPoint { x: 0.0, y: 0.0 }
        );
        assert_eq!(
            b.to_preview(DesktopPoint::new(0.0, 440.0)),
            PreviewPoint {
                x: 1920.0,
                y: 640.0
            }
        );
    }
}
