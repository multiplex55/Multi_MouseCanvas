use super::{
    model::CanvasModel,
    preview::{self, TilePreviewCache},
};
use crate::settings::model::PreviewOptions;
use egui::{Ui, Vec2};
use std::cell::RefCell;
thread_local! { static PREVIEW_CACHE: RefCell<TilePreviewCache> = RefCell::new(TilePreviewCache::default()); }
pub fn render_preview(ui: &mut Ui, canvas: &CanvasModel) {
    render_preview_sized(
        ui,
        canvas,
        &PreviewOptions::default(),
        Vec2::new(ui.available_width(), ui.available_height().max(240.0)),
    );
}
pub fn render_preview_sized(
    ui: &mut Ui,
    canvas: &CanvasModel,
    options: &PreviewOptions,
    available: Vec2,
) {
    PREVIEW_CACHE.with(|c| preview::render(ui, canvas, options, available, &mut c.borrow_mut()));
}
