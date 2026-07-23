use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CanvasPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VirtualDesktopBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl VirtualDesktopBounds {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_displays(displays: &[DisplayBounds]) -> Option<Self> {
        let first = displays.first()?;
        Some(displays.iter().skip(1).fold(
            Self::new(first.min_x, first.min_y, first.max_x, first.max_y),
            |acc, display| Self {
                min_x: acc.min_x.min(display.min_x),
                min_y: acc.min_y.min(display.min_y),
                max_x: acc.max_x.max(display.max_x),
                max_y: acc.max_y.max(display.max_y),
            },
        ))
    }

    pub fn width(&self) -> f32 {
        (self.max_x - self.min_x).max(1.0)
    }
    pub fn height(&self) -> f32 {
        (self.max_y - self.min_y).max(1.0)
    }

    pub fn normalize(&self, physical_x: f32, physical_y: f32) -> CanvasPoint {
        CanvasPoint {
            x: (physical_x - self.min_x) / self.width(),
            y: (physical_y - self.min_y) / self.height(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DisplayBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl DisplayBounds {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_virtual_desktop_coordinates_normalize_correctly() {
        let bounds = VirtualDesktopBounds::new(-1920.0, -200.0, 1920.0, 1080.0);
        assert_eq!(
            bounds.normalize(-1920.0, -200.0),
            CanvasPoint { x: 0.0, y: 0.0 }
        );
        assert_eq!(bounds.normalize(0.0, 440.0), CanvasPoint { x: 0.5, y: 0.5 });
    }

    #[test]
    fn primary_monitor_not_at_minimum_origin_normalizes_correctly() {
        let displays = [
            DisplayBounds::new(0.0, 0.0, 1920.0, 1080.0),
            DisplayBounds::new(-1280.0, -720.0, 0.0, 0.0),
        ];
        let bounds = VirtualDesktopBounds::from_displays(&displays).unwrap();
        assert_eq!(
            bounds,
            VirtualDesktopBounds::new(-1280.0, -720.0, 1920.0, 1080.0)
        );
        assert_eq!(
            bounds.normalize(-1280.0, -720.0),
            CanvasPoint { x: 0.0, y: 0.0 }
        );
        assert_eq!(bounds.normalize(0.0, 0.0), CanvasPoint { x: 0.4, y: 0.4 });
    }
}
