use crate::{
    canvas::{coordinates::SessionDesktopBounds, topology::DisplayTopology},
    export::model::{ExportBackground, ExportScale, InformationPanels},
};
use image::{Rgba, RgbaImage};
pub const PANEL_HEIGHT: u32 = 96;
pub fn panel_height(p: &InformationPanels, scale: ExportScale) -> u32 {
    if p.enabled() {
        ((PANEL_HEIGHT as f32) * scale.ratio()).round().max(1.) as u32
    } else {
        0
    }
}
pub fn add_panel(
    mut artwork: RgbaImage,
    p: &InformationPanels,
    bg: &ExportBackground,
) -> RgbaImage {
    if !p.enabled() {
        return artwork;
    }
    let h = PANEL_HEIGHT;
    let mut out = RgbaImage::new(artwork.width(), artwork.height() + h);
    for px in out.pixels_mut() {
        *px = match bg {
            ExportBackground::Solid(c) => Rgba([c.r, c.g, c.b, c.a]),
            ExportBackground::Transparent => Rgba([0, 0, 0, 192]),
        };
    }
    image::imageops::overlay(&mut out, &artwork, 0, 0);
    artwork = out;
    artwork
}
pub fn draw_monitors(
    img: &mut RgbaImage,
    topology: &DisplayTopology,
    bounds: SessionDesktopBounds,
    scale: f32,
    labels: bool,
) {
    for m in &topology.monitors {
        let x = ((m.physical_rect.min_x - bounds.min_x) * scale).round() as i32;
        let y = ((m.physical_rect.min_y - bounds.min_y) * scale).round() as i32;
        let w = (m.physical_rect.width() * scale).round() as i32;
        let h = (m.physical_rect.height() * scale).round() as i32;
        for xx in x..x + w {
            put(img, xx, y);
            put(img, xx, y + h - 1)
        }
        for yy in y..y + h {
            put(img, x, yy);
            put(img, x + w - 1, yy)
        }
        if labels {
            for xx in x..(x + 20).min(x + w) {
                put(img, xx, y + 2)
            }
        }
    }
}
fn put(i: &mut RgbaImage, x: i32, y: i32) {
    if x >= 0 && y >= 0 && (x as u32) < i.width() && (y as u32) < i.height() {
        *i.get_pixel_mut(x as u32, y as u32) = Rgba([255, 255, 255, 180]);
    }
}
