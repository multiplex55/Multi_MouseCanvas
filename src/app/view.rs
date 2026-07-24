use super::{application_editor, dialogs, performance_view, settings_view, toolbar};
use crate::{app::state::AppState, canvas::renderer};
use eframe::egui;

pub fn show(ctx: &egui::Context, state: &mut AppState, confirm_exit: &mut bool) {
    egui::TopBottomPanel::top("recording_toolbar").show(ctx, |ui| toolbar::show(ui, state));
    egui::SidePanel::right("settings_panel")
        .resizable(true)
        .default_width(300.0)
        .show(ctx, |ui| {
            ui.heading("Settings");
            settings_view::show(ui, state);
            ui.separator();
            application_editor::show(ui, state);
        });
    egui::CentralPanel::default().show(ctx, |ui| {
        if let Some(message) = &state.status_message {
            ui.colored_label(egui::Color32::YELLOW, message);
        }
        ui.separator();
        let stats = &state.statistics;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Status: {:?}", state.recording_status));
            ui.separator();
            ui.label(format!("Samples: {}", stats.samples_recorded));
            ui.separator();
            ui.label(format!("Distance: {:.0}px", stats.total_cursor_distance));
            ui.separator();
            ui.label(format!("Movements: {}", stats.movement_segment_count));
            ui.separator();
            ui.label(format!("Dwells: {}", stats.finalized_dwell_count));
        });
        ui.separator();
        let height = (ui.available_height() - 36.0).max(240.0);
        renderer::render_preview_sized(
            ui,
            &state.canvas,
            &state.settings.preview_options,
            egui::vec2(ui.available_width(), height),
        );
        ui.collapsing("Performance", |ui| performance_view::show(ui, state));
        ui.collapsing("Capture status", |ui| {
            ui.label(match &state.current_cursor_sample {
                Some(s) => format!("Cursor sample: ({:.1}, {:.1})", s.physical_x, s.physical_y),
                None => "Cursor sample: none".to_owned(),
            });
            ui.label(format!(
                "Foreground application: {}",
                state.current_foreground_application.label()
            ));
        });
    });
    dialogs::show(ctx, state, confirm_exit);
}
