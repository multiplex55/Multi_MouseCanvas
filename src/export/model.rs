use crate::{
    app_colors::registry::ApplicationColorRegistry, session::statistics::SessionStatistics,
    settings::model::RgbaColor,
};
use std::{path::PathBuf, time::SystemTime};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    WebP,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScale {
    Full,
    SeventyFive,
    Fifty,
    TwentyFive,
}
impl ExportScale {
    pub fn ratio(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::SeventyFive => 0.75,
            Self::Fifty => 0.5,
            Self::TwentyFive => 0.25,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExportBackground {
    Solid(RgbaColor),
    Transparent,
}
#[derive(Debug, Clone, Default)]
pub struct InformationPanels {
    pub application_legend: bool,
    pub session_times: bool,
    pub recording_duration: bool,
    pub total_distance: bool,
    pub dwell_count: bool,
    pub monitor_outlines: bool,
    pub monitor_labels: bool,
}
impl InformationPanels {
    pub fn enabled(&self) -> bool {
        self.application_legend
            || self.session_times
            || self.recording_duration
            || self.total_distance
            || self.dwell_count
    }
}
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub destination: Option<PathBuf>,
    pub default_directory: PathBuf,
    pub timestamp: SystemTime,
    pub format: ExportFormat,
    pub scale: ExportScale,
    pub background: ExportBackground,
    pub panels: InformationPanels,
    pub statistics: SessionStatistics,
    pub application_colors: ApplicationColorRegistry,
    pub started_at: Option<SystemTime>,
    pub ended_at: Option<SystemTime>,
}
impl ExportOptions {
    pub fn basic(dir: PathBuf) -> Self {
        Self {
            destination: None,
            default_directory: dir,
            timestamp: SystemTime::now(),
            format: ExportFormat::Png,
            scale: ExportScale::Full,
            background: ExportBackground::Transparent,
            panels: Default::default(),
            statistics: Default::default(),
            application_colors: Default::default(),
            started_at: None,
            ended_at: None,
        }
    }
}
