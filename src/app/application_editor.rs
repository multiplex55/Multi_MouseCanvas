use crate::{
    app::state::{AppState, SettingsUpdate},
    settings::model::RgbaColor,
};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.collapsing("Application colors", |ui| {
        let mut enabled = state.settings.app_specific_coloring_enabled;
        if ui.checkbox(&mut enabled, "Enable app coloring").changed() {
            state.apply_settings_update(SettingsUpdate::AppColoringEnabled(enabled));
        }
        ui.label("Changes affect future artwork only; existing tiles are not recolored.");
        let ids: Vec<String> = state
            .settings
            .application_colors
            .rules
            .keys()
            .cloned()
            .collect();
        egui::Grid::new("app_rules").striped(true).show(ui, |ui| {
            ui.label("Label");
            ui.label("Keys");
            ui.label("Color");
            ui.label("Merge into first");
            ui.end_row();
            for id in ids {
                let Some(rule) = state.settings.application_colors.rules.get(&id).cloned() else {
                    continue;
                };
                let mut label = rule.label.clone();
                if ui.text_edit_singleline(&mut label).lost_focus() && label != rule.label {
                    state.apply_settings_update(SettingsUpdate::AppRuleRename(id.clone(), label));
                }
                ui.label(rule.match_keys.join(", "));
                let mut color: egui::Color32 = (&rule.resolved_color()).into();
                if ui.color_edit_button_srgba(&mut color).changed() {
                    state.apply_settings_update(SettingsUpdate::AppRuleColor(
                        id.clone(),
                        RgbaColor::new(color.r(), color.g(), color.b(), color.a()),
                    ));
                }
                if ui.button("Merge").clicked() {
                    if let Some(first) = state
                        .settings
                        .application_colors
                        .rules
                        .keys()
                        .next()
                        .cloned()
                    {
                        if first != id {
                            state.apply_settings_update(SettingsUpdate::AppRuleMerge {
                                survivor: first,
                                merged: id.clone(),
                            });
                        }
                    }
                }
                ui.end_row();
            }
        });
    });
}
