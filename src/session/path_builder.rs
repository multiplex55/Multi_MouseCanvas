use crate::{
    canvas::{coordinates::CanvasPoint, model::MovementPath},
    capture::foreground::ApplicationIdentity,
    settings::model::RgbaColor,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PathStyle {
    pub color: RgbaColor,
    pub width: f32,
    pub opacity: f32,
    pub application: ApplicationIdentity,
}

#[derive(Debug, Clone)]
pub struct PathBuilder {
    min_distance: f32,
    collinear_epsilon: f32,
    max_points: usize,
    points: Vec<CanvasPoint>,
    style: PathStyle,
    discarded: u64,
    accepted: u64,
}

impl PathBuilder {
    pub fn new(min_distance: f32, style: PathStyle) -> Self {
        Self {
            min_distance,
            collinear_epsilon: 0.1,
            max_points: 2048,
            points: Vec::new(),
            style,
            discarded: 0,
            accepted: 0,
        }
    }
    pub fn push(&mut self, p: CanvasPoint) -> bool {
        if let Some(last) = self.points.last().copied() {
            if distance(last, p) < self.min_distance {
                self.discarded += 1;
                return false;
            }
        }
        self.points.push(p);
        self.accepted += 1;
        self.simplify_tail();
        self.points.len() >= self.max_points
    }
    fn simplify_tail(&mut self) {
        while self.points.len() >= 3 {
            let n = self.points.len();
            let a = self.points[n - 3];
            let b = self.points[n - 2];
            let c = self.points[n - 1];
            if nearly_collinear(a, b, c, self.collinear_epsilon) {
                self.points.remove(n - 2);
                self.discarded += 1;
            } else {
                break;
            }
        }
    }
    pub fn active_path(&self) -> Option<MovementPath> {
        if self.points.len() < 2 {
            return None;
        }
        let mut p = MovementPath::new(self.style.color.clone(), self.style.width, false);
        p.color.a = ((p.color.a as f32) * self.style.opacity.clamp(0.0, 1.0)) as u8;
        p.application = self.style.application.clone();
        p.points = self.points.clone();
        Some(p)
    }
    pub fn flush(&mut self) -> Option<MovementPath> {
        let mut p = self.active_path()?;
        p.finalized = true;
        self.points.clear();
        Some(p)
    }
    pub fn len(&self) -> usize {
        self.points.len()
    }
    pub fn accepted_points(&self) -> u64 {
        self.accepted
    }
    pub fn discarded_points(&self) -> u64 {
        self.discarded
    }
}
fn distance(a: CanvasPoint, b: CanvasPoint) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}
fn nearly_collinear(a: CanvasPoint, b: CanvasPoint, c: CanvasPoint, eps: f32) -> bool {
    let ab = (b.x - a.x, b.y - a.y);
    let bc = (c.x - b.x, c.y - b.y);
    let cross = (ab.0 * bc.1 - ab.1 * bc.0).abs();
    cross <= eps * (distance(a, b) + distance(b, c)).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::coordinates::CanvasPoint;
    #[test]
    fn retains_turns_but_drops_collinear() {
        let style = PathStyle {
            color: RgbaColor::new(1, 2, 3, 255),
            width: 1.0,
            opacity: 1.0,
            application: Default::default(),
        };
        let mut b = PathBuilder::new(0.5, style);
        for p in [(0., 0.), (1., 0.), (2., 0.), (2., 1.), (2., 2.)] {
            b.push(CanvasPoint { x: p.0, y: p.1 });
        }
        let path = b.active_path().unwrap();
        assert!(path.points.contains(&CanvasPoint { x: 2.0, y: 0.0 }));
        assert!(path.points.len() < 5);
    }
}
