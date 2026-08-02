use super::{
    application_editor, dialogs, monitor_selection, performance_view, settings_view, toolbar,
};
use crate::{app::state::AppState, canvas::renderer};
use eframe::egui;

pub fn show(
    ctx: &egui::Context,
    state: &mut AppState,
    lifecycle: &mut crate::app::lifecycle::LifecycleCoordinator,
) {
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
        if let Some(error) = &state.persistent_engine_error {
            ui.colored_label(egui::Color32::RED, error);
            if ui.button("Retry recording").clicked() {
                state.retry_engine_requested = true;
            }
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
            state.canvas(),
            &state.settings.preview_options,
            egui::vec2(ui.available_width(), height),
        );
        ui.collapsing("Performance", |ui| performance_view::show(ui, state));
        ui.collapsing("Capture status", |ui| {
            if let Some(h) = &state.capture_health {
                ui.label(format!("Engine: {:?}", h.engine));
                ui.label(format!("Sampler: {:?}", h.sampler));
                ui.label(format!(
                    "Last sample: {}",
                    h.since_last_observed_sample
                        .map(|d| format!("{:.1}s ago", d.as_secs_f32()))
                        .unwrap_or_else(|| "never".into())
                ));
                ui.label(format!(
                    "Snapshot age: {}",
                    state
                        .snapshot_received_at
                        .map(|t| format!("{:.1}s", t.elapsed().as_secs_f32()))
                        .unwrap_or_else(|| "never".into())
                ));
            }
        });
    });
    dialogs::show(ctx, state, lifecycle);
    monitor_selection::show(ctx, state);
}
