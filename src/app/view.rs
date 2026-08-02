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
        if let Some(error) = state.export_error.clone() {
            ui.colored_label(egui::Color32::RED, format!("Export failed: {error}"));
            if ui.button("Retry export").clicked() {
                state.retry_export();
            }
        }
        if let Some(crate::session::events::ExportResult::Success { path, .. }) =
            state.export_result.clone()
        {
            ui.label(format!("Saved: {}", path.display()));
            if ui.button("Open folder").clicked() {
                if let Err(e) = crate::platform::open_path::open_export_location(&path) {
                    state.export_error = Some(e.to_string());
                }
            }
        }
        ui.separator();
        let stats = &state.statistics;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Status: {:?}", state.recording_status));
            if let Some(pending) = &state.pending_transition {
                ui.label(format!("Pending: {:?}", pending.kind));
            }
            if state.export_busy() {
                ui.label("Activity: Exporting");
            }
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
                let healthy = match state.recording_status {
                    crate::session::model::RecordingStatus::Recording => {
                        h.sampler == crate::session::snapshot::SamplerState::Running
                            && h.since_last_observed_sample
                                .is_some_and(|age| age < std::time::Duration::from_secs(2))
                    }
                    crate::session::model::RecordingStatus::Paused
                    | crate::session::model::RecordingStatus::Finished
                    | crate::session::model::RecordingStatus::Stopped => {
                        h.sampler == crate::session::snapshot::SamplerState::Stopped
                    }
                };
                ui.label(if healthy {
                    "Capture: healthy"
                } else {
                    "Capture: attention required"
                });
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
