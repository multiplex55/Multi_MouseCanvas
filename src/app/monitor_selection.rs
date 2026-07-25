use crate::{
    app::state::AppState, canvas::topology::DisplayTopology, display_profiles::SavedDisplayProfile,
};
use eframe::egui;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MonitorSelectionState {
    pub detected: DisplayTopology,
    pub name: String,
    pub selected: Vec<String>,
    pub remember: bool,
    pub error: Option<String>,
}
impl MonitorSelectionState {
    pub fn new(detected: DisplayTopology) -> Self {
        let selected = detected
            .monitors
            .iter()
            .map(|m| m.stable_key().to_owned())
            .collect::<Vec<_>>();
        let name = crate::display_profiles::generated_name(&detected, &selected);
        Self {
            detected,
            name,
            selected,
            remember: true,
            error: None,
        }
    }
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut selection) = state.monitor_selection.take() else {
        return;
    };
    let mut open = true;
    let mut start = false;
    let mut cancel = false;
    egui::Window::new("Select monitors")
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Profile name");
            ui.text_edit_singleline(&mut selection.name);
            for m in &selection.detected.monitors {
                let key = m.stable_key().to_owned();
                let mut checked = selection.selected.contains(&key);
                let primary = if m.primary { " (Primary)" } else { "" };
                if ui
                    .checkbox(
                        &mut checked,
                        format!(
                            "{}{} — {:.0}×{:.0} at ({:.0}, {:.0})",
                            m.label.as_deref().unwrap_or(&m.id),
                            primary,
                            m.width,
                            m.height,
                            m.physical_rect.min_x,
                            m.physical_rect.min_y
                        ),
                    )
                    .changed()
                {
                    if checked {
                        selection.selected.push(key)
                    } else {
                        selection.selected.retain(|v| v != &key)
                    }
                }
            }
            ui.checkbox(&mut selection.remember, "Remember this profile");
            if let Some(e) = &selection.error {
                ui.colored_label(egui::Color32::RED, e);
            }
            ui.horizontal(|ui| {
                start = ui
                    .add_enabled(!selection.selected.is_empty(), egui::Button::new("Start"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if start {
        match crate::platform::display::current_topology() {
            Ok(current)
                if current.fingerprint == selection.detected.fingerprint
                    && selection
                        .selected
                        .iter()
                        .all(|k| current.monitors.iter().any(|m| m.stable_key() == k)) =>
            {
                let id = format!(
                    "profile-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                );
                let mut saved =
                    SavedDisplayProfile::from_topology(id, &current, selection.selected.clone());
                saved.name = selection.name;
                saved.renamed = true;
                let snapshot = saved.snapshot(&current).expect("validated selection");
                if selection.remember {
                    state.display_profiles.upsert(saved);
                    if let Some(path) = &state.display_profiles_path {
                        if let Err(e) = state.display_profiles.save(path) {
                            state.status_message = Some(format!("Profile could not be saved: {e}"))
                        }
                    }
                }
                state.canvas.current_topology = snapshot.effective_topology.clone();
                state.active_display_profile = Some(Arc::new(snapshot));
                state.start_recording();
                return;
            }
            Ok(current) => {
                selection = MonitorSelectionState::new(current);
                selection.error =
                    Some("The display layout changed. Review the refreshed selection.".into())
            }
            Err(_) => {
                selection.error =
                    Some("Displays could not be rechecked; recording was not started.".into())
            }
        }
    }
    if cancel || !open {
        state.status_message = Some("Recording canceled.".into())
    } else {
        state.monitor_selection = Some(selection)
    }
}
