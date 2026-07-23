use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl From<&RgbaColor> for egui::Color32 {
    fn from(value: &RgbaColor) -> Self {
        egui::Color32::from_rgba_unmultiplied(value.r, value.g, value.b, value.a)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub sampling_interval_ms: u64,
    pub movement_threshold_px: f32,
    pub dwell_activation_delay_ms: u64,
    pub dwell_growth_rate: f32,
    pub line_width_px: f32,
    pub default_movement_color: RgbaColor,
    pub default_dwell_color: RgbaColor,
    pub background_color: RgbaColor,
    pub app_specific_coloring_enabled: bool,
    pub export_directory: PathBuf,
    pub start_recording_automatically: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            sampling_interval_ms: 16,
            movement_threshold_px: 2.0,
            dwell_activation_delay_ms: 600,
            dwell_growth_rate: 1.25,
            line_width_px: 2.0,
            default_movement_color: RgbaColor::new(0, 120, 215, 255),
            default_dwell_color: RgbaColor::new(255, 185, 0, 255),
            background_color: RgbaColor::new(24, 24, 24, 255),
            app_specific_coloring_enabled: true,
            export_directory: PathBuf::from("exports"),
            start_recording_automatically: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_values_are_stable() {
        let settings = AppSettings::default();
        assert_eq!(settings.sampling_interval_ms, 16);
        assert_eq!(settings.movement_threshold_px, 2.0);
        assert_eq!(settings.dwell_activation_delay_ms, 600);
        assert_eq!(settings.dwell_growth_rate, 1.25);
        assert_eq!(settings.line_width_px, 2.0);
        assert_eq!(
            settings.default_movement_color,
            RgbaColor::new(0, 120, 215, 255)
        );
        assert_eq!(
            settings.default_dwell_color,
            RgbaColor::new(255, 185, 0, 255)
        );
        assert_eq!(settings.background_color, RgbaColor::new(24, 24, 24, 255));
        assert!(settings.app_specific_coloring_enabled);
        assert_eq!(settings.export_directory, PathBuf::from("exports"));
        assert!(!settings.start_recording_automatically);
    }

    #[test]
    fn loading_settings_json_tolerates_unknown_fields() {
        let json = r#"{
            "sampling_interval_ms": 20,
            "movement_threshold_px": 3.5,
            "dwell_activation_delay_ms": 700,
            "dwell_growth_rate": 1.5,
            "line_width_px": 4.0,
            "default_movement_color": { "r": 1, "g": 2, "b": 3, "a": 255 },
            "default_dwell_color": { "r": 4, "g": 5, "b": 6, "a": 255 },
            "background_color": { "r": 7, "g": 8, "b": 9, "a": 255 },
            "app_specific_coloring_enabled": false,
            "export_directory": "custom_exports",
            "start_recording_automatically": true,
            "future_field": "ignored"
        }"#;

        let settings: AppSettings = serde_json::from_str(json).expect("unknown fields are ignored");
        assert_eq!(settings.sampling_interval_ms, 20);
        assert_eq!(settings.export_directory, PathBuf::from("custom_exports"));
        assert!(settings.start_recording_automatically);
    }
}
