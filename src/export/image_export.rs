use crate::{
    canvas::model::{CanvasModel, DwellShape, MovementPath},
    settings::model::{DwellRenderMode, DwellShapeKind, RgbaColor},
};
use image::{Rgba, RgbaImage};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportBackground {
    Solid,
    Transparent,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub destination: Option<PathBuf>,
    pub default_directory: PathBuf,
    pub session_name: Option<String>,
    pub timestamp: SystemTime,
    pub custom_size: Option<ExportSize>,
    pub background: ExportBackground,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("failed to create export directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("failed to write PNG {path}: {source}")]
    WritePng {
        path: PathBuf,
        source: image::ImageError,
    },
}

pub fn export_png(canvas: &CanvasModel, options: &ExportOptions) -> Result<PathBuf, ExportError> {
    let path = match &options.destination {
        Some(path) => path.clone(),
        None => collision_safe_path(&options.default_directory, &base_filename(options), "png"),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ExportError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let image = render_canvas_to_image(canvas, options);
    image.save(&path).map_err(|source| ExportError::WritePng {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub fn render_canvas_to_image(canvas: &CanvasModel, options: &ExportOptions) -> RgbaImage {
    let size = export_size(canvas, options.custom_size.clone());
    let mut img = RgbaImage::new(size.width, size.height);
    let bg = match options.background {
        ExportBackground::Solid => rgba(&canvas.background.color),
        ExportBackground::Transparent => Rgba([
            canvas.background.color.r,
            canvas.background.color.g,
            canvas.background.color.b,
            0,
        ]),
    };
    for p in img.pixels_mut() {
        *p = bg;
    }
    let sx = size.width as f32 / canvas.dimensions.0.max(1.0);
    let sy = size.height as f32 / canvas.dimensions.1.max(1.0);
    for path in canvas
        .finalized_movement_paths
        .iter()
        .chain(canvas.active_movement_segment.iter())
    {
        draw_path(&mut img, path, sx, sy);
    }
    for shape in canvas
        .finalized_dwell_shapes
        .iter()
        .chain(canvas.active_dwell_shape.iter())
    {
        draw_dwell(&mut img, shape, sx, sy);
    }
    img
}

pub fn export_size(canvas: &CanvasModel, custom: Option<ExportSize>) -> ExportSize {
    custom.unwrap_or_else(|| ExportSize {
        width: canvas.dimensions.0.round().max(1.0) as u32,
        height: canvas.dimensions.1.round().max(1.0) as u32,
    })
}

pub fn base_filename(options: &ExportOptions) -> String {
    let name = options
        .session_name
        .as_deref()
        .map(sanitize_filename_component)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "session".to_owned());
    format!("{}_{}", name, timestamp_component(options.timestamp))
}

pub fn sanitize_filename_component(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else if ch.is_whitespace()
            || matches!(
                ch,
                '.' | '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        {
            if !out.ends_with('_') {
                out.push('_');
            }
        }
    }
    out.trim_matches('_').to_owned()
}

pub fn collision_safe_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut n = 0;
    loop {
        let filename = if n == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{n}.{extension}")
        };
        let path = dir.join(filename);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

fn timestamp_component(ts: SystemTime) -> String {
    format!(
        "{}",
        ts.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    )
}
fn rgba(c: &RgbaColor) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, c.a])
}
fn blend(dst: &mut Rgba<u8>, src: Rgba<u8>) {
    let a = src[3] as f32 / 255.0;
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a) + (dst[i] as f32 * (1.0 - a))).round() as u8;
    }
    dst[3] = ((src[3] as f32) + (dst[3] as f32 * (1.0 - a))).round() as u8;
}
fn put(img: &mut RgbaImage, x: i32, y: i32, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        blend(img.get_pixel_mut(x as u32, y as u32), c);
    }
}
fn draw_path(img: &mut RgbaImage, path: &MovementPath, sx: f32, sy: f32) {
    for pair in path.points.windows(2) {
        line(
            img,
            (pair[0].x * sx) as i32,
            (pair[0].y * sy) as i32,
            (pair[1].x * sx) as i32,
            (pair[1].y * sy) as i32,
            (path.width * ((sx + sy) / 2.0)).max(1.0) as i32,
            rgba(&path.color),
        );
    }
}
fn line(img: &mut RgbaImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, w: i32, c: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        disc(img, x0, y0, w / 2, c);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
fn draw_dwell(img: &mut RgbaImage, s: &DwellShape, sx: f32, sy: f32) {
    let cx = (s.center.x * sx) as i32;
    let cy = (s.center.y * sy) as i32;
    let r = (s.size * ((sx + sy) / 2.0) / 2.0).max(1.0) as i32;
    let mut fill = rgba(&s.color);
    fill[3] = ((fill[3] as f32) * s.fill_opacity.clamp(0.0, 1.0)) as u8;
    let stroke = rgba(&s.color);
    match s.shape_kind {
        DwellShapeKind::Circle => {
            if s.render_mode != DwellRenderMode::Outline {
                disc(img, cx, cy, r, fill)
            }
            if s.render_mode != DwellRenderMode::Fill {
                circle_outline(img, cx, cy, r, s.outline_width as i32, stroke)
            }
        }
        DwellShapeKind::Square => rect_shape(img, cx, cy, r, s, fill, stroke),
        DwellShapeKind::Triangle => triangle(img, cx, cy, r, s, fill, stroke),
    }
}
fn disc(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, c: Rgba<u8>) {
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            if (x - cx).pow(2) + (y - cy).pow(2) <= r.pow(2) {
                put(img, x, y, c)
            }
        }
    }
}
fn circle_outline(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, w: i32, c: Rgba<u8>) {
    let inner = (r - w.max(1)).max(0);
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            let d = (x - cx).pow(2) + (y - cy).pow(2);
            if d <= r.pow(2) && d >= inner.pow(2) {
                put(img, x, y, c)
            }
        }
    }
}
fn rect_shape(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    r: i32,
    s: &DwellShape,
    fill: Rgba<u8>,
    stroke: Rgba<u8>,
) {
    if s.render_mode != DwellRenderMode::Outline {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                put(img, x, y, fill)
            }
        }
    }
    if s.render_mode != DwellRenderMode::Fill {
        let w = s.outline_width.max(1.0) as i32;
        for i in 0..w {
            for x in cx - r..=cx + r {
                put(img, x, cy - r + i, stroke);
                put(img, x, cy + r - i, stroke)
            }
            for y in cy - r..=cy + r {
                put(img, cx - r + i, y, stroke);
                put(img, cx + r - i, y, stroke)
            }
        }
    }
}
fn triangle(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    r: i32,
    s: &DwellShape,
    fill: Rgba<u8>,
    stroke: Rgba<u8>,
) {
    let pts = [(cx, cy - r), (cx - r, cy + r), (cx + r, cy + r)];
    if s.render_mode != DwellRenderMode::Outline {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let a = edge(pts[0], pts[1], (x, y));
                let b = edge(pts[1], pts[2], (x, y));
                let c = edge(pts[2], pts[0], (x, y));
                if (a >= 0 && b >= 0 && c >= 0) || (a <= 0 && b <= 0 && c <= 0) {
                    put(img, x, y, fill)
                }
            }
        }
    }
    if s.render_mode != DwellRenderMode::Fill {
        line(
            img,
            pts[0].0,
            pts[0].1,
            pts[1].0,
            pts[1].1,
            s.outline_width as i32,
            stroke,
        );
        line(
            img,
            pts[1].0,
            pts[1].1,
            pts[2].0,
            pts[2].1,
            s.outline_width as i32,
            stroke,
        );
        line(
            img,
            pts[2].0,
            pts[2].1,
            pts[0].0,
            pts[0].1,
            s.outline_width as i32,
            stroke,
        );
    }
}
fn edge(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> i32 {
    (p.0 - a.0) * (b.1 - a.1) - (p.1 - a.1) * (b.0 - a.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{coordinates::CanvasPoint, model::MovementPath};
    use tempfile::tempdir;
    fn opts(dir: &Path) -> ExportOptions {
        ExportOptions {
            destination: None,
            default_directory: dir.to_path_buf(),
            session_name: Some("My Session:/1".into()),
            timestamp: UNIX_EPOCH + std::time::Duration::from_secs(42),
            custom_size: None,
            background: ExportBackground::Transparent,
        }
    }
    #[test]
    fn collision_safe_filename_generation() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("a.png"), b"").unwrap();
        assert_eq!(
            collision_safe_path(d.path(), "a", "png")
                .file_name()
                .unwrap(),
            "a-1.png"
        );
    }
    #[test]
    fn timestamp_session_name_filename_sanitization() {
        let o = opts(Path::new("x"));
        assert_eq!(sanitize_filename_component(" A:/B*C? "), "A_B_C");
        assert_eq!(base_filename(&o), "My_Session_1_42");
    }
    #[test]
    fn export_uses_logical_canvas_dimensions_not_preview() {
        let c = CanvasModel {
            dimensions: (321.0, 123.0),
            ..Default::default()
        };
        let img = render_canvas_to_image(&c, &opts(Path::new("x")));
        assert_eq!((img.width(), img.height()), (321, 123));
    }
    #[test]
    fn transparent_export_preserves_alpha() {
        let mut c = CanvasModel::default();
        c.dimensions = (2.0, 2.0);
        c.background.color = RgbaColor::new(1, 2, 3, 255);
        let img = render_canvas_to_image(&c, &opts(Path::new("x")));
        assert_eq!(img.get_pixel(0, 0)[3], 0);
    }
    #[test]
    fn export_png_writes_collision_safe_file() {
        let d = tempdir().unwrap();
        let c = CanvasModel::default();
        let p = export_png(&c, &opts(d.path())).unwrap();
        assert!(p.exists());
    }
    #[test]
    fn active_state_is_rendered() {
        let mut c = CanvasModel {
            dimensions: (10.0, 10.0),
            ..Default::default()
        };
        let mut p = MovementPath::new(RgbaColor::new(255, 0, 0, 255), 1.0, false);
        p.points = vec![
            CanvasPoint { x: 0.0, y: 0.0 },
            CanvasPoint { x: 9.0, y: 0.0 },
        ];
        c.active_movement_segment = Some(p);
        let img = render_canvas_to_image(&c, &opts(Path::new("x")));
        assert_eq!(img.get_pixel(5, 0)[0], 255);
    }
}
