use super::{
    coordinates::DesktopPoint,
    model::{DwellShape, MovementPath},
    tiles::SparseTileStore,
};
use crate::settings::model::{DwellRenderMode, DwellShapeKind, RgbaColor};

fn rgba(c: &RgbaColor) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}
pub fn source_over(dst: &mut [u8], src: [u8; 4]) {
    let a = src[3] as f32 / 255.0;
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a) + (dst[i] as f32 * (1.0 - a))).round() as u8;
    }
    dst[3] = ((src[3] as f32) + (dst[3] as f32 * (1.0 - a))).round() as u8;
}
fn put(store: &mut SparseTileStore, x: i32, y: i32, c: [u8; 4]) {
    store.put_pixel(x, y, c, source_over);
}
pub fn rasterize_movement_path(store: &mut SparseTileStore, path: &MovementPath) {
    for w in path.points.windows(2) {
        rasterize_line(store, w[0], w[1], path.width, rgba(&path.color));
    }
}
pub fn rasterize_line(
    store: &mut SparseTileStore,
    a: DesktopPoint,
    b: DesktopPoint,
    width: f32,
    color: [u8; 4],
) {
    let steps = ((b.x - a.x).abs().max((b.y - a.y).abs())).ceil().max(1.0) as i32;
    let r = (width.max(1.0) / 2.0).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (a.x + (b.x - a.x) * t).round() as i32;
        let y = (a.y + (b.y - a.y) * t).round() as i32;
        disc(store, x, y, r, color);
    }
}
fn disc(store: &mut SparseTileStore, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r {
                put(store, x, y, c);
            }
        }
    }
}
pub fn rasterize_dwell_shape(store: &mut SparseTileStore, s: &DwellShape) {
    let cx = s.center.x.round() as i32;
    let cy = s.center.y.round() as i32;
    let r = (s.size / 2.0).max(1.0) as i32;
    let mut fill = rgba(&s.color);
    fill[3] = ((fill[3] as f32) * s.fill_opacity.clamp(0.0, 1.0)) as u8;
    let stroke = rgba(&s.color);
    match s.shape_kind {
        DwellShapeKind::Circle => {
            if s.render_mode != DwellRenderMode::Outline {
                disc(store, cx, cy, r, fill)
            }
            if s.render_mode != DwellRenderMode::Fill {
                circle_outline(store, cx, cy, r, s.outline_width.max(1.0) as i32, stroke)
            }
        }
        DwellShapeKind::Square => square(store, cx, cy, r, s, fill, stroke),
        DwellShapeKind::Triangle => triangle(store, cx, cy, r, s, fill, stroke),
    }
}
fn circle_outline(store: &mut SparseTileStore, cx: i32, cy: i32, r: i32, w: i32, c: [u8; 4]) {
    let inner = (r - w).max(0);
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            if d <= r * r && d >= inner * inner {
                put(store, x, y, c);
            }
        }
    }
}
fn square(
    store: &mut SparseTileStore,
    cx: i32,
    cy: i32,
    r: i32,
    s: &DwellShape,
    fill: [u8; 4],
    stroke: [u8; 4],
) {
    if s.render_mode != DwellRenderMode::Outline {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                put(store, x, y, fill)
            }
        }
    }
    if s.render_mode != DwellRenderMode::Fill {
        let w = s.outline_width.max(1.0) as i32;
        for i in 0..w {
            for x in cx - r..=cx + r {
                put(store, x, cy - r + i, stroke);
                put(store, x, cy + r - i, stroke)
            }
            for y in cy - r..=cy + r {
                put(store, cx - r + i, y, stroke);
                put(store, cx + r - i, y, stroke)
            }
        }
    }
}
fn triangle(
    store: &mut SparseTileStore,
    cx: i32,
    cy: i32,
    r: i32,
    s: &DwellShape,
    fill: [u8; 4],
    stroke: [u8; 4],
) {
    let pts = [
        DesktopPoint::new(cx as f32, (cy - r) as f32),
        DesktopPoint::new((cx - r) as f32, (cy + r) as f32),
        DesktopPoint::new((cx + r) as f32, (cy + r) as f32),
    ];
    if s.render_mode != DwellRenderMode::Outline {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let a = edge(pts[0], pts[1], x, y);
                let b = edge(pts[1], pts[2], x, y);
                let c = edge(pts[2], pts[0], x, y);
                if (a >= 0.0 && b >= 0.0 && c >= 0.0) || (a <= 0.0 && b <= 0.0 && c <= 0.0) {
                    put(store, x, y, fill)
                }
            }
        }
    }
    if s.render_mode != DwellRenderMode::Fill {
        rasterize_line(store, pts[0], pts[1], s.outline_width, stroke);
        rasterize_line(store, pts[1], pts[2], s.outline_width, stroke);
        rasterize_line(store, pts[2], pts[0], s.outline_width, stroke);
    }
}
fn edge(a: DesktopPoint, b: DesktopPoint, x: i32, y: i32) -> f32 {
    (x as f32 - a.x) * (b.y - a.y) - (y as f32 - a.y) * (b.x - a.x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::model::DwellShape,
        settings::model::{DwellRenderMode, DwellShapeKind, RgbaColor},
    };
    use std::time::Duration;
    #[test]
    fn drawing_across_tile_boundaries() {
        let mut s = SparseTileStore::default();
        rasterize_line(
            &mut s,
            DesktopPoint::new(250.0, 1.0),
            DesktopPoint::new(260.0, 1.0),
            1.0,
            [255, 0, 0, 255],
        );
        assert_eq!(s.touched_tile_count(), 2);
    }
    #[test]
    fn alpha_compositing_of_overlapping_shapes() {
        let mut s = SparseTileStore::default();
        let a = DwellShape {
            center: DesktopPoint::new(10.0, 10.0),
            duration: Duration::ZERO,
            size: 10.0,
            color: RgbaColor::new(255, 0, 0, 128),
            shape_kind: DwellShapeKind::Circle,
            fill_opacity: 1.0,
            outline_width: 1.0,
            render_mode: DwellRenderMode::Fill,
            finalized: true,
            application: Default::default(),
        };
        let mut b = a.clone();
        b.color = RgbaColor::new(0, 0, 255, 128);
        rasterize_dwell_shape(&mut s, &a);
        rasterize_dwell_shape(&mut s, &b);
        let (tc, lp) = crate::canvas::tiles::desktop_pixel_to_tile(10, 10, 256);
        let t = s.tiles.get(&tc).unwrap();
        let i = ((lp.y as usize * 256 + lp.x as usize) * 4) as usize;
        assert!(t.pixels[i] > 0 && t.pixels[i + 2] > 0);
    }
}
