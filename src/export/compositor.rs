use crate::{
    canvas::model::CanvasModel,
    export::{
        model::{ExportBackground, ExportOptions},
        overlays,
    },
};
use image::{Rgba, RgbaImage};
pub fn artwork_dimensions(canvas: &CanvasModel, options: &ExportOptions) -> (u32, u32) {
    let r = options.scale.ratio();
    (
        (canvas.session_desktop_bounds.width() * r).round().max(1.) as u32,
        (canvas.session_desktop_bounds.height() * r).round().max(1.) as u32,
    )
}
pub fn compose(canvas: &CanvasModel, options: &ExportOptions) -> RgbaImage {
    let (w, h) = artwork_dimensions(canvas, options);
    let bg = match &options.background {
        ExportBackground::Solid(c) => Rgba([c.r, c.g, c.b, c.a]),
        ExportBackground::Transparent => Rgba([0, 0, 0, 0]),
    };
    let mut full = RgbaImage::from_pixel(
        canvas.session_desktop_bounds.width().round().max(1.) as u32,
        canvas.session_desktop_bounds.height().round().max(1.) as u32,
        bg,
    );
    let b = canvas.session_desktop_bounds;
    let size = canvas.sparse_tiles.tile_size as i32;
    for (coord, tile) in &canvas.sparse_tiles.tiles {
        for ty in 0..size {
            let ay = coord.y * size + ty;
            if (ay as f32) < b.min_y || (ay as f32) >= b.max_y {
                continue;
            }
            for tx in 0..size {
                let ax = coord.x * size + tx;
                if (ax as f32) < b.min_x || (ax as f32) >= b.max_x {
                    continue;
                }
                let idx = ((ty * size + tx) * 4) as usize;
                let src = Rgba([
                    tile.pixels[idx],
                    tile.pixels[idx + 1],
                    tile.pixels[idx + 2],
                    tile.pixels[idx + 3],
                ]);
                if src[3] == 0 {
                    continue;
                }
                let dx = (ax as f32 - b.min_x) as u32;
                let dy = (ay as f32 - b.min_y) as u32;
                blend(full.get_pixel_mut(dx, dy), src)
            }
        }
    }
    let mut art = if full.dimensions() == (w, h) {
        full
    } else {
        image::imageops::resize(&full, w, h, image::imageops::FilterType::Lanczos3)
    };
    if options.panels.monitor_outlines {
        overlays::draw_monitors(
            &mut art,
            &canvas.current_topology,
            b,
            options.scale.ratio(),
            options.panels.monitor_labels,
        )
    }
    overlays::add_panel(art, &options.panels, &options.background)
}
fn blend(d: &mut Rgba<u8>, s: Rgba<u8>) {
    let sa = s[3] as u32;
    let da = d[3] as u32;
    let oa = sa + da * (255 - sa) / 255;
    if oa == 0 {
        *d = Rgba([0, 0, 0, 0]);
        return;
    }
    for n in 0..3 {
        d[n] = ((s[n] as u32 * sa + d[n] as u32 * da * (255 - sa) / 255) / oa) as u8
    }
    d[3] = oa as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::coordinates::DesktopRect,
        export::model::{ExportScale, InformationPanels},
        settings::model::RgbaColor,
    };
    fn options(scale: ExportScale, bg: ExportBackground) -> ExportOptions {
        let mut o = ExportOptions::basic(".".into());
        o.scale = scale;
        o.background = bg;
        o
    }
    #[test]
    fn scales_preserve_aspect_ratio() {
        let mut c = CanvasModel::default();
        c.session_desktop_bounds = DesktopRect::new(0., 0., 400., 200.);
        for (s, w, h) in [
            (ExportScale::Full, 400, 200),
            (ExportScale::SeventyFive, 300, 150),
            (ExportScale::Fifty, 200, 100),
            (ExportScale::TwentyFive, 100, 50),
        ] {
            assert_eq!(
                artwork_dimensions(&c, &options(s, ExportBackground::Transparent)),
                (w, h)
            );
        }
    }
    #[test]
    fn transparent_and_solid_untouched_pixels() {
        let mut c = CanvasModel::default();
        c.session_desktop_bounds = DesktopRect::new(0., 0., 2., 2.);
        assert_eq!(
            compose(
                &c,
                &options(ExportScale::Full, ExportBackground::Transparent)
            )
            .get_pixel(0, 0)[3],
            0
        );
        assert_eq!(
            *compose(
                &c,
                &options(
                    ExportScale::Full,
                    ExportBackground::Solid(RgbaColor::new(1, 2, 3, 255))
                )
            )
            .get_pixel(0, 0),
            Rgba([1, 2, 3, 255])
        );
    }
    #[test]
    fn negative_origin_and_edge_crop() {
        let mut c = CanvasModel::default();
        c.session_desktop_bounds = DesktopRect::new(-1., -1., 1., 1.);
        c.sparse_tiles
            .put_pixel(-1, -1, [9, 8, 7, 255], |d, s| d.copy_from_slice(&s));
        c.sparse_tiles
            .put_pixel(1, 1, [1, 1, 1, 255], |d, s| d.copy_from_slice(&s));
        let i = compose(
            &c,
            &options(ExportScale::Full, ExportBackground::Transparent),
        );
        assert_eq!(*i.get_pixel(0, 0), Rgba([9, 8, 7, 255]));
        assert_eq!(i.dimensions(), (2, 2));
    }
    #[test]
    fn metadata_panel_is_outside_artwork() {
        let c = CanvasModel::default();
        let mut o = options(ExportScale::Full, ExportBackground::Transparent);
        o.panels = InformationPanels {
            session_times: true,
            ..Default::default()
        };
        let i = compose(&c, &o);
        assert_eq!(
            i.height(),
            c.session_desktop_bounds.height() as u32 + crate::export::overlays::PANEL_HEIGHT
        );
    }
}
