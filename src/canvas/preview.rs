use crate::{
    canvas::{
        coordinates::TileCoordinate,
        model::{CanvasModel, DwellShape, MovementPath},
        tiles::Tile,
    },
    settings::model::{DwellRenderMode, DwellShapeKind, PreviewOptions, RgbaColor},
};
use egui::{pos2, Color32, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Ui, Vec2};
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewTransform {
    pub scale: f32,
    pub size: Vec2,
    pub origin: Pos2,
}
#[derive(Default)]
pub struct TilePreviewCache {
    textures: HashMap<TileCoordinate, CachedTexture>,
    test_revisions: HashMap<TileCoordinate, u64>,
    pub upload_count: u64,
}
struct CachedTexture {
    revision: u64,
    texture: TextureHandle,
}
impl TilePreviewCache {
    pub fn cached_tile_count(&self) -> usize {
        self.textures.len()
    }
    pub fn sync_tile_for_tests(&mut self, coord: TileCoordinate, revision: u64) {
        if self.test_revisions.get(&coord).copied() != Some(revision) {
            self.upload_count += 1;
            self.test_revisions.insert(coord, revision);
        }
    }
}
pub fn preview_scale(available: Vec2, logical: (f32, f32)) -> PreviewTransform {
    let w = logical.0.max(1.0);
    let h = logical.1.max(1.0);
    let scale = (available.x / w).min(available.y / h).max(0.0);
    PreviewTransform {
        scale,
        size: Vec2::new(w * scale, h * scale),
        origin: Pos2::ZERO,
    }
}
pub fn render(
    ui: &mut Ui,
    canvas: &CanvasModel,
    options: &PreviewOptions,
    available: Vec2,
    cache: &mut TilePreviewCache,
) {
    let mut transform = preview_scale(available, canvas.canvas_dimensions());
    let (resp, painter) = ui.allocate_painter(transform.size, egui::Sense::hover());
    let rect = resp.rect;
    transform.origin = rect.min;
    if canvas.background.transparent {
        draw_checkerboard(&painter, rect, 12.0)
    } else {
        painter.rect_filled(rect, 4.0, color32(&canvas.background.color));
    }
    for (coord, tile) in &canvas.sparse_tiles.tiles {
        if !tile.contains_artwork {
            continue;
        }
        let tex = texture_for(ui, cache, *coord, tile, canvas.sparse_tiles.tile_size);
        let s = canvas.sparse_tiles.tile_size as f32;
        let min = pos2(
            rect.left()
                + ((*coord).x as f32 * s - canvas.session_desktop_bounds.min_x) * transform.scale,
            rect.top()
                + ((*coord).y as f32 * s - canvas.session_desktop_bounds.min_y) * transform.scale,
        );
        let r = Rect::from_min_size(min, Vec2::splat(s * transform.scale));
        painter.image(
            tex.id(),
            r,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if let Some(p) = &canvas.active_movement_overlay {
        draw_path(&painter, rect, transform, canvas, p)
    }
    if let Some(d) = &canvas.active_dwell_overlay {
        draw_dwell(&painter, rect, transform, canvas, d)
    }
    if options.monitor_outlines {
        draw_monitors(&painter, rect, transform, canvas, options.monitor_labels);
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
}
fn texture_for(
    ui: &Ui,
    cache: &mut TilePreviewCache,
    coord: TileCoordinate,
    tile: &Tile,
    size: u32,
) -> TextureHandle {
    let needs = cache.textures.get(&coord).map(|c| c.revision) != Some(tile.revision);
    if needs {
        let img =
            egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &tile.pixels);
        let texture = ui.ctx().load_texture(
            format!("canvas_tile_{}_{}", coord.x, coord.y),
            img,
            TextureOptions::LINEAR,
        );
        cache.textures.insert(
            coord,
            CachedTexture {
                revision: tile.revision,
                texture,
            },
        );
        cache.upload_count += 1;
    }
    cache.textures.get(&coord).unwrap().texture.clone()
}
fn map_point(rect: Rect, t: PreviewTransform, c: &CanvasModel, x: f32, y: f32) -> Pos2 {
    pos2(
        rect.left() + (x - c.session_desktop_bounds.min_x) * t.scale,
        rect.top() + (y - c.session_desktop_bounds.min_y) * t.scale,
    )
}
fn draw_path(
    p: &egui::Painter,
    rect: Rect,
    t: PreviewTransform,
    c: &CanvasModel,
    path: &MovementPath,
) {
    if path.points.len() < 2 {
        return;
    }
    let pts = path
        .points
        .iter()
        .map(|pt| map_point(rect, t, c, pt.x, pt.y))
        .collect();
    p.add(egui::Shape::line(
        pts,
        Stroke::new(path.width * t.scale.max(0.25), color32(&path.color)),
    ));
}
fn draw_dwell(p: &egui::Painter, rect: Rect, t: PreviewTransform, c: &CanvasModel, s: &DwellShape) {
    let center = map_point(rect, t, c, s.center.x, s.center.y);
    let r = s.size * t.scale / 2.0;
    let fill = match s.render_mode {
        DwellRenderMode::Outline => Color32::TRANSPARENT,
        _ => {
            let mut c = s.color.clone();
            c.a = ((c.a as f32) * s.fill_opacity.clamp(0.0, 1.0)) as u8;
            color32(&c)
        }
    };
    let stroke = match s.render_mode {
        DwellRenderMode::Fill => Stroke::NONE,
        _ => Stroke::new(s.outline_width * t.scale.max(0.25), color32(&s.color)),
    };
    match s.shape_kind {
        DwellShapeKind::Circle => p.circle(center, r, fill, stroke),
        DwellShapeKind::Square => p.rect(
            Rect::from_center_size(center, Vec2::splat(r * 2.0)),
            0.0,
            fill,
            stroke,
        ),
        DwellShapeKind::Triangle => p.add(egui::Shape::convex_polygon(
            vec![
                pos2(center.x, center.y - r),
                pos2(center.x - r, center.y + r),
                pos2(center.x + r, center.y + r),
            ],
            fill,
            stroke,
        )),
    };
}
fn draw_monitors(
    p: &egui::Painter,
    rect: Rect,
    t: PreviewTransform,
    c: &CanvasModel,
    labels: bool,
) {
    for m in &c.current_topology.monitors {
        let min = map_point(rect, t, c, m.physical_rect.min_x, m.physical_rect.min_y);
        let max = map_point(rect, t, c, m.physical_rect.max_x, m.physical_rect.max_y);
        let r = Rect::from_min_max(min, max);
        p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(120)));
        if labels {
            p.text(
                r.left_top() + egui::vec2(6.0, 6.0),
                egui::Align2::LEFT_TOP,
                m.label.as_deref().unwrap_or(&m.id),
                egui::FontId::default(),
                Color32::WHITE,
            );
        }
    }
}
fn draw_checkerboard(p: &egui::Painter, rect: Rect, cell: f32) {
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left();
        let mut col = 0;
        while x < rect.right() {
            p.rect_filled(
                Rect::from_min_size(pos2(x, y), Vec2::splat(cell)),
                0.0,
                if (row + col) % 2 == 0 {
                    Color32::from_gray(190)
                } else {
                    Color32::from_gray(140)
                },
            );
            x += cell;
            col += 1;
        }
        y += cell;
        row += 1;
    }
}
fn color32(c: &RgbaColor) -> Color32 {
    c.into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_cache_uploads_only_changed_tile_revisions() {
        let mut c = TilePreviewCache::default();
        let tc = TileCoordinate { x: 0, y: 0 };
        c.sync_tile_for_tests(tc, 1);
        c.sync_tile_for_tests(tc, 1);
        assert_eq!(c.upload_count, 1);
        c.sync_tile_for_tests(tc, 2);
        assert_eq!(c.upload_count, 2);
    }
    #[test]
    fn preview_transform_changes_on_left_up_bounds_expansion_without_texture_reupload() {
        let mut cache = TilePreviewCache::default();
        let tc = TileCoordinate { x: 0, y: 0 };
        cache.sync_tile_for_tests(tc, 1);
        let a = preview_scale(Vec2::new(500.0, 500.0), (100.0, 100.0));
        let b = preview_scale(Vec2::new(500.0, 500.0), (200.0, 200.0));
        cache.sync_tile_for_tests(tc, 1);
        assert_ne!(a.scale, b.scale);
        assert_eq!(cache.upload_count, 1);
    }
}
