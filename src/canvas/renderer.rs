use super::model::CanvasModel;
use egui::{Color32, Frame, Sense, Stroke, Ui, Vec2};

pub fn render_preview(ui: &mut Ui, canvas: &CanvasModel, background: Color32) {
    let desired_size = Vec2::new(ui.available_width(), 320.0);
    let (response, painter) = ui.allocate_painter(desired_size, Sense::hover());
    let rect = response.rect;
    painter.rect_filled(rect, 6.0, background);
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::DARK_GRAY));

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
