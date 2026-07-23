use super::coordinates::CanvasPoint;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CanvasModel {
    pub points: Vec<CanvasPoint>,
}

impl CanvasModel {
    pub fn clear(&mut self) {
        self.points.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}
