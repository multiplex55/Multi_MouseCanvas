use crate::{app::state::AppState, session::model::RecordingStatus};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let editable = matches!(state.recording_status, RecordingStatus::Stopped);
    let mut delete = None;
    let mut prefer = None;
    for profile in &mut state.display_profiles.profiles {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if editable {
                    if ui.text_edit_singleline(&mut profile.name).changed() {
                        profile.renamed = true
                    }
                } else {
                    ui.label(&profile.name);
                };
                if profile.preferred {
                    ui.label("Preferred");
                }
            });
            ui.label(format!(
                "{} selected of {} detected",
                profile.included_stable_keys.len(),
                profile.detected_monitors.len()
            ));
            for monitor in &profile.detected_monitors {
                let mut included = profile.included_stable_keys.contains(&monitor.stable_key);
                if ui
                    .add_enabled(
                        editable,
                        egui::Checkbox::new(&mut included, &monitor.display_name),
                    )
                    .changed()
                {
                    if included {
                        profile
                            .included_stable_keys
                            .push(monitor.stable_key.clone());
                    } else if profile.included_stable_keys.len() > 1 {
                        profile
                            .included_stable_keys
                            .retain(|key| key != &monitor.stable_key);
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(editable, egui::Button::new("Set preferred"))
                    .clicked()
                {
                    prefer = Some(profile.id.clone())
                }
                if ui
                    .add_enabled(editable, egui::Button::new("Delete"))
                    .clicked()
                {
                    delete = Some(profile.id.clone())
                }
            });
        });
    }
    if let Some(id) = prefer {
        state.display_profiles.set_preferred(&id);
    }
    if let Some(id) = delete {
        state.display_profiles.delete(&id);
    }
    if ui
        .add_enabled(editable, egui::Button::new("Forget current layout"))
        .clicked()
    {
        state
            .display_profiles
            .forget_layout(&state.canvas.current_topology);
    }
    if editable {
        if let Some(path) = &state.display_profiles_path {
            let _ = state.display_profiles.save(path);
        }
    }
}
