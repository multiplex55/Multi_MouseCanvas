use crate::{
    app::state::{AppState, SettingsUpdate},
    settings::model::{DwellRenderMode, DwellShapeKind, PreviewFitBehavior},
};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.collapsing("Recording", |ui| {
        let mut v = state.settings.sampling_interval_ms;
        if ui
            .add(egui::Slider::new(&mut v, 1..=1000).text("Sampling ms"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::SamplingIntervalMs(v));
        }
        let mut t = state.settings.movement_threshold_px;
        if ui
            .add(egui::Slider::new(&mut t, 0.0..=100.0).text("Movement threshold"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::MovementThresholdPx(t));
        }
        let mut r = state.settings.dwell_tolerance_radius_px;
        if ui
            .add(egui::Slider::new(&mut r, 1.0..=200.0).text("Dwell tolerance"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::DwellToleranceRadiusPx(r));
        }
        let mut d = state.settings.dwell_activation_delay_ms;
        if ui
            .add(egui::Slider::new(&mut d, 0..=10000).text("Dwell delay ms"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::DwellActivationDelayMs(d));
        }
    });
    ui.collapsing("Canvas", |ui| {
        color(ui, "Background", &mut state.settings.background_color);
        if ui.button("Apply background").clicked() {
            state.apply_settings_update(SettingsUpdate::CanvasVisuals)
        }
        let mut tr = state.settings.transparent_canvas_mode;
        if ui.checkbox(&mut tr, "Transparent mode").changed() {
            state.apply_settings_update(SettingsUpdate::TransparentCanvasMode(tr));
        }
        let mut outlines = state.settings.preview_options.monitor_outlines;
        if ui.checkbox(&mut outlines, "Monitor outlines").changed() {
            state.apply_settings_update(SettingsUpdate::MonitorOutlines(outlines));
        }
        let mut labels = state.settings.preview_options.monitor_labels;
        if ui.checkbox(&mut labels, "Monitor labels").changed() {
            state.apply_settings_update(SettingsUpdate::MonitorLabels(labels));
        }
        egui::ComboBox::from_label("Preview fit")
            .selected_text(format!("{:?}", state.settings.preview_fit_behavior))
            .show_ui(ui, |ui| {
                let mut fit = state.settings.preview_fit_behavior;
                for f in [
                    PreviewFitBehavior::FitAll,
                    PreviewFitBehavior::FillAvailable,
                ] {
                    if ui
                        .selectable_value(&mut fit, f, format!("{:?}", f))
                        .changed()
                    {
                        state.apply_settings_update(SettingsUpdate::PreviewFitBehavior(fit));
                    }
                }
            });
    });
    ui.collapsing("Movement paths", |ui| {
        let mut w = state.settings.line_width_px;
        if ui
            .add(egui::Slider::new(&mut w, 0.1..=64.0).text("Line width"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::LineWidthPx(w));
        }
        let mut o = state.settings.line_opacity;
        if ui
            .add(egui::Slider::new(&mut o, 0.0..=1.0).text("Line opacity"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::LineOpacity(o));
        }
    });
    ui.collapsing("Dwell shapes", |ui| {
        egui::ComboBox::from_label("Shape")
            .selected_text(format!("{:?}", state.settings.selected_dwell_shape))
            .show_ui(ui, |ui| {
                let mut s = state.settings.selected_dwell_shape;
                for k in [
                    DwellShapeKind::Circle,
                    DwellShapeKind::Triangle,
                    DwellShapeKind::Square,
                ] {
                    if ui.selectable_value(&mut s, k, format!("{:?}", k)).changed() {
                        state.apply_settings_update(SettingsUpdate::DwellShapeKind(s));
                    }
                }
            });
        let mut min = state.settings.min_dwell_shape_size;
        if ui
            .add(egui::Slider::new(&mut min, 1.0..=512.0).text("Minimum size"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::MinDwellSize(min));
        }
        let mut max = state.settings.max_dwell_shape_size;
        if ui
            .add(egui::Slider::new(&mut max, 1.0..=1024.0).text("Maximum size"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::MaxDwellSize(max));
        }
        let mut g = state.settings.dwell_growth_rate;
        if ui
            .add(egui::Slider::new(&mut g, 0.0..=100.0).text("Growth rate"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::DwellGrowthRate(g));
        }
        let mut fo = state.settings.dwell_fill_opacity;
        if ui
            .add(egui::Slider::new(&mut fo, 0.0..=1.0).text("Fill opacity"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::DwellFillOpacity(fo));
        }
        let mut ow = state.settings.dwell_outline_width;
        if ui
            .add(egui::Slider::new(&mut ow, 0.0..=64.0).text("Outline width"))
            .changed()
        {
            state.apply_settings_update(SettingsUpdate::DwellOutlineWidth(ow));
        }
        egui::ComboBox::from_label("Render mode")
            .selected_text(format!("{:?}", state.settings.dwell_render_mode))
            .show_ui(ui, |ui| {
                let mut m = state.settings.dwell_render_mode;
                for x in [
                    DwellRenderMode::Fill,
                    DwellRenderMode::Outline,
                    DwellRenderMode::FillAndOutline,
                ] {
                    if ui.selectable_value(&mut m, x, format!("{:?}", x)).changed() {
                        state.apply_settings_update(SettingsUpdate::DwellRenderMode(m));
                    }
                }
            });
    });
    ui.collapsing("Export", |ui| {
        ui.label(format!(
            "Directory: {}",
            state.settings.export_directory.display()
        ));
        ui.label(format!(
            "Format: {:?}, scale: {:.1}",
            state.settings.export_format, state.settings.export_scale
        ));
    });
}
fn color(ui: &mut egui::Ui, label: &str, c: &mut crate::settings::model::RgbaColor) {
    let mut ec: egui::Color32 = (&*c).into();
    if ui.color_edit_button_srgba(&mut ec).changed() {
        *c = crate::settings::model::RgbaColor::new(ec.r(), ec.g(), ec.b(), ec.a());
    }
    ui.label(label);
}
