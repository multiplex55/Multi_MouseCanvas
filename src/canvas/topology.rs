use super::coordinates::{DesktopPoint, DesktopRect, MonitorBounds, SessionDesktopBounds};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    pub id: String,
    pub physical_rect: MonitorBounds,
    pub width: f32,
    pub height: f32,
    pub primary: bool,
    pub relative_position: DesktopPoint,
    pub label: Option<String>,
}
impl Monitor {
    pub fn new(id: impl Into<String>, rect: MonitorBounds, primary: bool) -> Self {
        Self {
            id: id.into(),
            physical_rect: rect,
            width: rect.width(),
            height: rect.height(),
            primary,
            relative_position: DesktopPoint::new(rect.min_x, rect.min_y),
            label: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologySignature(pub String);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayTopology {
    pub monitors: Vec<Monitor>,
    pub signature: TopologySignature,
}
impl DisplayTopology {
    pub fn new(monitors: Vec<Monitor>) -> Self {
        let mut parts: Vec<String> = monitors
            .iter()
            .map(|m| {
                format!(
                    "{}:{:.0},{:.0},{:.0},{:.0}:{}",
                    m.id,
                    m.physical_rect.min_x,
                    m.physical_rect.min_y,
                    m.physical_rect.max_x,
                    m.physical_rect.max_y,
                    m.primary
                )
            })
            .collect();
        parts.sort();
        Self {
            monitors,
            signature: TopologySignature(parts.join("|")),
        }
    }
    pub fn bounds(&self) -> Option<SessionDesktopBounds> {
        let rects: Vec<_> = self.monitors.iter().map(|m| m.physical_rect).collect();
        DesktopRect::union_all(&rects)
    }
    pub fn monitor_containing(&self, p: DesktopPoint) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.physical_rect.contains(p))
    }
}
impl Default for DisplayTopology {
    fn default() -> Self {
        Self::new(vec![Monitor::new(
            "primary",
            DesktopRect::new(0.0, 0.0, 1920.0, 1080.0),
            true,
        )])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopologyHistory {
    pub entries: Vec<DisplayTopology>,
}
impl TopologyHistory {
    pub fn record_if_changed(&mut self, topology: DisplayTopology) -> bool {
        if self
            .entries
            .last()
            .is_none_or(|t| t.signature != topology.signature)
        {
            self.entries.push(topology);
            true
        } else {
            false
        }
    }
}

pub fn expand_session_bounds(
    current: SessionDesktopBounds,
    topology: &DisplayTopology,
) -> SessionDesktopBounds {
    topology
        .bounds()
        .map(|b| current.union(&b))
        .unwrap_or(current)
}
pub fn positions_on_different_monitors(
    topology: &DisplayTopology,
    a: DesktopPoint,
    b: DesktopPoint,
) -> bool {
    topology.monitor_containing(a).map(|m| &m.id) != topology.monitor_containing(b).map(|m| &m.id)
}
pub fn segment_crosses_uncovered_space(
    topology: &DisplayTopology,
    a: DesktopPoint,
    b: DesktopPoint,
) -> bool {
    let steps = (((b.x - a.x).abs().max((b.y - a.y).abs())) / 8.0)
        .ceil()
        .max(1.0) as i32;
    (0..=steps).any(|i| {
        let t = i as f32 / steps as f32;
        topology
            .monitor_containing(DesktopPoint::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
            ))
            .is_none()
    })
}
pub fn clip_to_monitor_boundary(m: &Monitor, from: DesktopPoint, to: DesktopPoint) -> DesktopPoint {
    let mut last = from;
    for i in 1..=256 {
        let t = i as f32 / 256.0;
        let p = DesktopPoint::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        if !m.physical_rect.contains(p) {
            return last;
        }
        last = p;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    fn m(id: &str, r: DesktopRect) -> Monitor {
        Monitor::new(id, r, id == "p")
    }
    #[test]
    fn single_ultrawide_monitor_bounds() {
        let t = DisplayTopology::new(vec![m("p", DesktopRect::new(0.0, 0.0, 3440.0, 1440.0))]);
        assert_eq!(t.bounds().unwrap().width(), 3440.0);
    }
    #[test]
    fn monitor_left_of_primary_with_negative_x() {
        let t = DisplayTopology::new(vec![
            m("l", DesktopRect::new(-1280.0, 0.0, 0.0, 1024.0)),
            m("p", DesktopRect::new(0.0, 0.0, 1920.0, 1080.0)),
        ]);
        assert_eq!(t.bounds().unwrap().min_x, -1280.0);
    }
    #[test]
    fn monitor_above_primary_with_negative_y() {
        let t = DisplayTopology::new(vec![
            m("a", DesktopRect::new(0.0, -900.0, 1600.0, 0.0)),
            m("p", DesktopRect::new(0.0, 0.0, 1920.0, 1080.0)),
        ]);
        assert_eq!(t.bounds().unwrap().min_y, -900.0);
    }
    #[test]
    fn three_unevenly_aligned_monitors() {
        let t = DisplayTopology::new(vec![
            m("l", DesktopRect::new(-1000.0, 100.0, 0.0, 900.0)),
            m("p", DesktopRect::new(0.0, 0.0, 1920.0, 1080.0)),
            m("r", DesktopRect::new(1920.0, -200.0, 3200.0, 824.0)),
        ]);
        assert_eq!(
            t.bounds().unwrap(),
            DesktopRect::new(-1000.0, -200.0, 3200.0, 1080.0)
        );
    }
    #[test]
    fn empty_rectangular_gaps() {
        let t = DisplayTopology::new(vec![
            m("p", DesktopRect::new(0.0, 0.0, 100.0, 100.0)),
            m("r", DesktopRect::new(200.0, 0.0, 300.0, 100.0)),
        ]);
        assert!(segment_crosses_uncovered_space(
            &t,
            DesktopPoint::new(50.0, 50.0),
            DesktopPoint::new(250.0, 50.0)
        ));
    }
    #[test]
    fn origin_expansion_leftward_and_upward() {
        let b = DesktopRect::new(0.0, 0.0, 100.0, 100.0);
        let t = DisplayTopology::new(vec![m("n", DesktopRect::new(-50.0, -50.0, 10.0, 10.0))]);
        assert_eq!(
            expand_session_bounds(b, &t),
            DesktopRect::new(-50.0, -50.0, 100.0, 100.0)
        );
    }
    #[test]
    fn monitor_removal_without_shrinking_session_bounds() {
        let old = DesktopRect::new(-100.0, 0.0, 100.0, 100.0);
        let t = DisplayTopology::new(vec![m("p", DesktopRect::new(0.0, 0.0, 100.0, 100.0))]);
        assert_eq!(expand_session_bounds(old, &t), old);
    }
    #[test]
    fn rotation_resolution_change_signature_updates() {
        let a = DisplayTopology::new(vec![m("p", DesktopRect::new(0.0, 0.0, 100.0, 200.0))]);
        let b = DisplayTopology::new(vec![m("p", DesktopRect::new(0.0, 0.0, 200.0, 100.0))]);
        assert_ne!(a.signature, b.signature);
    }
}
