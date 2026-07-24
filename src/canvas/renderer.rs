use super::model::{CanvasModel, DwellShape, MovementPath};
use crate::settings::model::{DwellRenderMode, DwellShapeKind, RgbaColor};
use egui::{pos2, Color32, Frame, Pos2, Rect, Sense, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewTransform {
    pub scale: f32,
    pub size: Vec2,
}

pub fn preview_scale(available: Vec2, logical: (f32, f32)) -> PreviewTransform {
    let logical_w = logical.0.max(1.0);
    let logical_h = logical.1.max(1.0);
    let scale = (available.x / logical_w)
        .min(available.y / logical_h)
        .max(0.0);
    PreviewTransform {
        scale,
        size: Vec2::new(logical_w * scale, logical_h * scale),
    }
}

pub fn render_preview(ui: &mut Ui, canvas: &CanvasModel) {
    let available = Vec2::new(ui.available_width(), 320.0);
    let transform = preview_scale(available, canvas.canvas_dimensions());
    let (response, painter) = ui.allocate_painter(transform.size, Sense::hover());
    let rect = response.rect;

    if canvas.background.transparent {
        draw_checkerboard(&painter, rect, 12.0);
    } else {
        painter.rect_filled(rect, 6.0, color32(&canvas.background.color));
    }
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::DARK_GRAY));

    if let Some(path) = &canvas.active_movement_overlay {
        draw_path(&painter, rect, transform.scale, path);
    }
    if let Some(shape) = &canvas.active_dwell_overlay {
        draw_dwell(&painter, rect, transform.scale, shape);
    }

    if canvas.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Canvas preview will appear here",
            egui::TextStyle::Body.resolve(ui.style()),
            Color32::GRAY,
        );
    }

    Frame::none().show(ui, |_| {});
}

fn draw_checkerboard(painter: &egui::Painter, rect: Rect, cell: f32) {
    let light = Color32::from_gray(190);
    let dark = Color32::from_gray(140);
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left();
        let mut col = 0;
        while x < rect.right() {
            let color = if (row + col) % 2 == 0 { light } else { dark };
            painter.rect_filled(
                Rect::from_min_size(pos2(x, y), Vec2::splat(cell).min(rect.size())),
                0.0,
                color,
            );
            x += cell;
            col += 1;
        }
        y += cell;
        row += 1;
    }
}

fn draw_path(painter: &egui::Painter, rect: Rect, scale: f32, path: &MovementPath) {
    if path.points.len() < 2 {
        return;
    }
    let points: Vec<Pos2> = path
        .points
        .iter()
        .map(|p| pos2(rect.left() + p.x * scale, rect.top() + p.y * scale))
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(path.width * scale.max(0.25), color32(&path.color)),
    ));
}

fn draw_dwell(painter: &egui::Painter, rect: Rect, scale: f32, shape: &DwellShape) {
    let center = pos2(
        rect.left() + shape.center.x * scale,
        rect.top() + shape.center.y * scale,
    );
    let radius = shape.size * scale / 2.0;
    let fill = fill_color(shape);
    let stroke = outline_stroke(shape, scale);
    match shape.shape_kind {
        DwellShapeKind::Circle => painter.circle(center, radius, fill, stroke),
        DwellShapeKind::Square => painter.rect(
            Rect::from_center_size(center, Vec2::splat(radius * 2.0)),
            0.0,
            fill,
            stroke,
        ),
        DwellShapeKind::Triangle => {
            let points = vec![
                pos2(center.x, center.y - radius),
                pos2(center.x - radius, center.y + radius),
                pos2(center.x + radius, center.y + radius),
            ];
            painter.add(egui::Shape::convex_polygon(points, fill, stroke))
        }
    };
}

fn fill_color(shape: &DwellShape) -> Color32 {
    match shape.render_mode {
        DwellRenderMode::Outline => Color32::TRANSPARENT,
        DwellRenderMode::Fill | DwellRenderMode::FillAndOutline => {
            let mut c = shape.color.clone();
            c.a = ((c.a as f32) * shape.fill_opacity.clamp(0.0, 1.0)) as u8;
            color32(&c)
        }
    }
}

fn outline_stroke(shape: &DwellShape, scale: f32) -> Stroke {
    match shape.render_mode {
        DwellRenderMode::Fill => Stroke::NONE,
        DwellRenderMode::Outline | DwellRenderMode::FillAndOutline => {
            Stroke::new(shape.outline_width * scale.max(0.25), color32(&shape.color))
        }
    }
}

fn color32(color: &RgbaColor) -> Color32 {
    color.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_scale_preserves_aspect_ratio() {
        let transform = preview_scale(Vec2::new(1000.0, 300.0), (1920.0, 1080.0));
        assert!((transform.size.x / transform.size.y - 16.0 / 9.0).abs() < 0.001);
        assert!(transform.size.x <= 1000.0);
        assert!(transform.size.y <= 300.0);
    }
}
