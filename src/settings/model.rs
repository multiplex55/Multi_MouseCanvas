use crate::{app::commands::CloseWindowBehavior, app_colors::registry::ApplicationColorRegistry};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DwellShapeKind {
    Circle,
    Triangle,
    Square,
}

impl Default for DwellShapeKind {
    fn default() -> Self {
        Self::Circle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DwellRenderMode {
    Fill,
    Outline,
    FillAndOutline,
}

impl Default for DwellRenderMode {
    fn default() -> Self {
        Self::FillAndOutline
    }
}

const fn default_min_dwell_shape_size() -> f32 {
    12.0
}
const fn default_max_dwell_shape_size() -> f32 {
    96.0
}
const fn default_dwell_fill_opacity() -> f32 {
    0.45
}
const fn default_dwell_outline_width() -> f32 {
    2.0
}
const fn default_movement_smoothing_enabled() -> Option<bool> {
    Some(true)
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
    #[serde(default)]
    pub application_colors: ApplicationColorRegistry,
    pub export_directory: PathBuf,
    pub start_recording_automatically: bool,
    #[serde(default)]
    pub selected_dwell_shape: DwellShapeKind,
    #[serde(default = "default_min_dwell_shape_size")]
    pub min_dwell_shape_size: f32,
    #[serde(default = "default_max_dwell_shape_size")]
    pub max_dwell_shape_size: f32,
    #[serde(default = "default_dwell_fill_opacity")]
    pub dwell_fill_opacity: f32,
    #[serde(default = "default_dwell_outline_width")]
    pub dwell_outline_width: f32,
    #[serde(default)]
    pub dwell_render_mode: DwellRenderMode,
    #[serde(default)]
    pub transparent_canvas_mode: bool,
    #[serde(default = "default_movement_smoothing_enabled")]
    pub movement_smoothing_enabled: Option<bool>,
    #[serde(default)]
    pub close_window_behavior: CloseWindowBehavior,
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
            application_colors: ApplicationColorRegistry::default(),
            export_directory: PathBuf::from("exports"),
            start_recording_automatically: false,
            selected_dwell_shape: DwellShapeKind::Circle,
            min_dwell_shape_size: 12.0,
            max_dwell_shape_size: 96.0,
            dwell_fill_opacity: 0.45,
            dwell_outline_width: 2.0,
            dwell_render_mode: DwellRenderMode::FillAndOutline,
            transparent_canvas_mode: false,
            movement_smoothing_enabled: Some(true),
            close_window_behavior: CloseWindowBehavior::default(),
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
        assert_eq!(settings.selected_dwell_shape, DwellShapeKind::Circle);
        assert_eq!(settings.min_dwell_shape_size, 12.0);
        assert_eq!(settings.max_dwell_shape_size, 96.0);
        assert_eq!(settings.dwell_fill_opacity, 0.45);
        assert_eq!(settings.dwell_outline_width, 2.0);
        assert_eq!(settings.dwell_render_mode, DwellRenderMode::FillAndOutline);
        assert!(!settings.transparent_canvas_mode);
        assert_eq!(settings.movement_smoothing_enabled, Some(true));
        assert_eq!(
            settings.close_window_behavior,
            CloseWindowBehavior::MinimizeToTrayWhileRecording
        );
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
        assert_eq!(settings.min_dwell_shape_size, 12.0);
        assert_eq!(settings.max_dwell_shape_size, 96.0);
        assert_eq!(settings.movement_smoothing_enabled, Some(true));
        assert_eq!(
            settings.close_window_behavior,
            CloseWindowBehavior::MinimizeToTrayWhileRecording
        );
    }
}
