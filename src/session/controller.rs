use crate::{
    capture::{foreground::ApplicationIdentity, sampler::CursorSample},
    settings::model::{AppSettings, RgbaColor},
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct MovementSegment {
    pub points: Vec<(f32, f32)>,
    pub application: ApplicationIdentity,
    pub color: RgbaColor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DwellEvent {
    pub start_time: Instant,
    pub center_x: f32,
    pub center_y: f32,
    pub duration: Duration,
    pub size: f32,
    pub visible: bool,
    pub application: ApplicationIdentity,
    pub color: RgbaColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscontinuityReason {
    PauseResume,
    LargeSampleGap,
    SuspectedSleepOrLock,
    DisplayConfigurationChanged,
}

#[derive(Debug, Clone)]
struct DwellCandidate {
    start_time: Instant,
    center_x: f32,
    center_y: f32,
    visible: bool,
    application: ApplicationIdentity,
    color: RgbaColor,
}

#[derive(Debug, Clone)]
pub struct MovementClassifier {
    threshold_px: f32,
    dwell_activation_delay: Duration,
    dwell_growth_rate: f32,
    min_dwell_size: f32,
    max_dwell_size: f32,
    large_gap: Duration,
    sleep_gap: Duration,
    previous_accepted: Option<CursorSample>,
    last_observed: Option<CursorSample>,
    movement_anchor: Option<CursorSample>,
    active_segment: Option<MovementSegment>,
    dwell_candidate: Option<DwellCandidate>,
    discontinuity_pending: bool,
    pub segments: Vec<MovementSegment>,
    pub dwells: Vec<DwellEvent>,
    pub total_distance: f32,
    pub active_position: Option<(f32, f32)>,
    current_application: ApplicationIdentity,
    current_color: RgbaColor,
}

impl MovementClassifier {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            threshold_px: settings.movement_threshold_px,
            dwell_activation_delay: Duration::from_millis(settings.dwell_activation_delay_ms),
            dwell_growth_rate: settings.dwell_growth_rate,
            min_dwell_size: settings.min_dwell_shape_size,
            max_dwell_size: settings.max_dwell_shape_size,
            large_gap: Duration::from_millis(
                settings.sampling_interval_ms.saturating_mul(10).max(250),
            ),
            sleep_gap: Duration::from_secs(30),
            previous_accepted: None,
            last_observed: None,
            movement_anchor: None,
            active_segment: None,
            dwell_candidate: None,
            discontinuity_pending: false,
            segments: Vec::new(),
            dwells: Vec::new(),
            total_distance: 0.0,
            active_position: None,
            current_application: ApplicationIdentity::default(),
            current_color: settings.default_movement_color.clone(),
        }
    }

    pub fn mark_discontinuity(&mut self, _reason: DiscontinuityReason) {
        self.finalize_active_segment();
        self.finalize_dwell();
        self.discontinuity_pending = true;
    }

    pub fn set_foreground_context(&mut self, application: ApplicationIdentity, color: RgbaColor) {
        if self.current_application.stable_key() != application.stable_key() {
            self.finalize_active_segment();
            self.finalize_dwell();
            self.discontinuity_pending = true;
            self.current_application = application;
        }
        self.current_color = color;
    }

    pub fn accept_sample(&mut self, sample: CursorSample) {
        if let Some(previous) = &self.last_observed {
            let gap = sample
                .timestamp
                .saturating_duration_since(previous.timestamp);
            if gap > self.sleep_gap {
                self.mark_discontinuity(DiscontinuityReason::SuspectedSleepOrLock);
            } else if gap > self.large_gap {
                self.mark_discontinuity(DiscontinuityReason::LargeSampleGap);
            }
        }

        if self.previous_accepted.is_none() || self.discontinuity_pending {
            self.anchor(sample);
            return;
        }

        let anchor = self
            .movement_anchor
            .clone()
            .unwrap_or_else(|| self.previous_accepted.clone().unwrap());
        let distance = distance(
            anchor.physical_x,
            anchor.physical_y,
            sample.physical_x,
            sample.physical_y,
        );
        if distance > self.threshold_px {
            self.finalize_dwell();
            self.extend_movement(&anchor, &sample, distance);
            self.previous_accepted = Some(sample.clone());
            self.movement_anchor = Some(sample.clone());
            self.last_observed = Some(sample.clone());
            self.active_position = Some((sample.physical_x, sample.physical_y));
        } else {
            self.last_observed = Some(sample.clone());
            self.update_dwell(&sample);
        }
    }

    fn anchor(&mut self, sample: CursorSample) {
        self.discontinuity_pending = false;
        self.previous_accepted = Some(sample.clone());
        self.last_observed = Some(sample.clone());
        self.movement_anchor = Some(sample.clone());
        self.active_position = Some((sample.physical_x, sample.physical_y));
        self.dwell_candidate = Some(DwellCandidate {
            start_time: sample.timestamp,
            center_x: sample.physical_x,
            center_y: sample.physical_y,
            visible: false,
            application: self.current_application.clone(),
            color: self.current_color.clone(),
        });
    }

    fn extend_movement(&mut self, previous: &CursorSample, sample: &CursorSample, distance: f32) {
        self.total_distance += distance;
        let segment = self.active_segment.get_or_insert_with(|| MovementSegment {
            points: vec![(previous.physical_x, previous.physical_y)],
            application: self.current_application.clone(),
            color: self.current_color.clone(),
        });
        segment.points.push((sample.physical_x, sample.physical_y));
    }

    fn update_dwell(&mut self, sample: &CursorSample) {
        if self.dwell_candidate.is_none() {
            self.dwell_candidate = Some(DwellCandidate {
                start_time: sample.timestamp,
                center_x: sample.physical_x,
                center_y: sample.physical_y,
                visible: false,
                application: self.current_application.clone(),
                color: self.current_color.clone(),
            });
        }
        if let Some(candidate) = &mut self.dwell_candidate {
            let duration = sample
                .timestamp
                .saturating_duration_since(candidate.start_time);
            if duration >= self.dwell_activation_delay {
                candidate.visible = true;
            }
        }
    }

    pub fn finalize_dwell(&mut self) {
        if let (Some(candidate), Some(previous)) =
            (self.dwell_candidate.take(), self.last_observed.as_ref())
        {
            let duration = previous
                .timestamp
                .saturating_duration_since(candidate.start_time);
            if candidate.visible {
                self.dwells.push(DwellEvent {
                    start_time: candidate.start_time,
                    center_x: candidate.center_x,
                    center_y: candidate.center_y,
                    duration,
                    size: (self.min_dwell_size + duration.as_secs_f32() * self.dwell_growth_rate)
                        .clamp(
                            self.min_dwell_size,
                            self.max_dwell_size.max(self.min_dwell_size),
                        ),
                    visible: true,
                    application: candidate.application,
                    color: candidate.color,
                });
            }
        }
    }

    pub fn finalize_active_segment(&mut self) {
        if let Some(segment) = self.active_segment.take() {
            if segment.points.len() > 1 {
                self.segments.push(segment);
            }
        }
    }

    pub fn current_dwell_visible(&self) -> bool {
        self.dwell_candidate.as_ref().is_some_and(|d| d.visible)
    }

    pub fn current_dwell_duration(&self) -> Duration {
        match (self.dwell_candidate.as_ref(), self.last_observed.as_ref()) {
            (Some(candidate), Some(previous)) if candidate.visible => previous
                .timestamp
                .saturating_duration_since(candidate.start_time),
            _ => Duration::ZERO,
        }
    }

    pub fn active_segment(&self) -> Option<&MovementSegment> {
        self.active_segment.as_ref()
    }

    pub fn active_dwell(&self) -> Option<DwellEvent> {
        let candidate = self.dwell_candidate.as_ref()?;
        let previous = self.last_observed.as_ref()?;
        if !candidate.visible {
            return None;
        }
        let duration = previous
            .timestamp
            .saturating_duration_since(candidate.start_time);
        Some(DwellEvent {
            start_time: candidate.start_time,
            center_x: candidate.center_x,
            center_y: candidate.center_y,
            duration,
            size: (self.min_dwell_size + duration.as_secs_f32() * self.dwell_growth_rate).clamp(
                self.min_dwell_size,
                self.max_dwell_size.max(self.min_dwell_size),
            ),
            visible: true,
            application: candidate.application.clone(),
            color: candidate.color.clone(),
        })
    }
}

fn distance(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t0: Instant, ms: u64, x: f32, y: f32) -> CursorSample {
        CursorSample::new(t0 + Duration::from_millis(ms), x, y)
    }

    fn classifier() -> MovementClassifier {
        let mut settings = AppSettings::default();
        settings.movement_threshold_px = 5.0;
        settings.dwell_activation_delay_ms = 100;
        MovementClassifier::new(&settings)
    }

    #[test]
    fn movement_below_threshold_does_not_create_segments() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 10, 3.0, 4.0));
        c.finalize_active_segment();
        assert!(c.segments.is_empty());
    }

    #[test]
    fn movement_above_threshold_creates_and_extends_segment() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 10, 6.0, 0.0));
        c.accept_sample(sample(t0, 20, 12.0, 0.0));
        c.finalize_active_segment();
        assert_eq!(c.segments.len(), 1);
        assert_eq!(c.segments[0].points.len(), 3);
        assert_eq!(c.total_distance, 12.0);
    }

    #[test]
    fn jitter_does_not_reset_dwell_before_delay() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 10.0, 10.0));
        c.accept_sample(sample(t0, 50, 12.0, 11.0));
        assert!(!c.current_dwell_visible());
        assert_eq!(c.dwell_candidate.as_ref().unwrap().start_time, t0);
    }

    #[test]
    fn dwell_becomes_visible_after_delay() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 10.0, 10.0));
        c.accept_sample(sample(t0, 100, 11.0, 11.0));
        assert!(c.current_dwell_visible());
    }

    #[test]
    fn foreground_application_changes_split_movement_segments_without_connecting_lines() {
        let t0 = Instant::now();
        let mut c = classifier();
        let app_a = ApplicationIdentity::new(1, "a.exe", Some("c:/a.exe".into()), None);
        let app_b = ApplicationIdentity::new(2, "b.exe", Some("c:/b.exe".into()), None);
        let blue = RgbaColor::new(1, 2, 3, 255);
        let red = RgbaColor::new(9, 8, 7, 255);
        c.set_foreground_context(app_a.clone(), blue.clone());
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 10, 10.0, 0.0));
        c.set_foreground_context(app_b.clone(), red.clone());
        c.accept_sample(sample(t0, 20, 1000.0, 1000.0));
        c.accept_sample(sample(t0, 30, 1010.0, 1000.0));
        c.finalize_active_segment();
        assert_eq!(c.segments.len(), 2);
        assert_eq!(c.segments[0].points, vec![(0.0, 0.0), (10.0, 0.0)]);
        assert_eq!(c.segments[0].application, app_a);
        assert_eq!(c.segments[1].points[0], (1000.0, 1000.0));
        assert_eq!(c.segments[1].application, app_b);
    }

    #[test]
    fn foreground_application_changes_finalize_active_dwell() {
        let t0 = Instant::now();
        let mut c = classifier();
        let app_a = ApplicationIdentity::new(1, "a.exe", Some("c:/a.exe".into()), None);
        let app_b = ApplicationIdentity::new(2, "b.exe", Some("c:/b.exe".into()), None);
        c.set_foreground_context(app_a.clone(), RgbaColor::new(1, 2, 3, 255));
        c.accept_sample(sample(t0, 0, 50.0, 50.0));
        c.accept_sample(sample(t0, 120, 51.0, 50.0));
        assert!(c.current_dwell_visible());
        c.set_foreground_context(app_b, RgbaColor::new(9, 8, 7, 255));
        assert_eq!(c.dwells.len(), 1);
        assert_eq!(c.dwells[0].application, app_a);
    }

    #[test]
    fn resume_discontinuity_does_not_draw_long_connecting_line() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.mark_discontinuity(DiscontinuityReason::DisplayConfigurationChanged);
        c.accept_sample(sample(t0, 10, 1000.0, 1000.0));
        c.finalize_active_segment();
        assert!(c.segments.is_empty());
        c.accept_sample(sample(t0, 20, 1010.0, 1000.0));
        c.finalize_active_segment();
        assert_eq!(c.segments.len(), 1);
        assert_eq!(c.segments[0].points[0], (1000.0, 1000.0));
    }
}

#[cfg(test)]
mod dwell_contract_tests {
    use super::*;
    fn sample(t0: Instant, ms: u64, x: f32, y: f32) -> CursorSample {
        CursorSample::new(t0 + Duration::from_millis(ms), x, y)
    }
    fn classifier() -> MovementClassifier {
        let mut s = AppSettings::default();
        s.movement_threshold_px = 5.0;
        s.dwell_activation_delay_ms = 100;
        s.dwell_growth_rate = 10.0;
        s.min_dwell_shape_size = 4.0;
        s.max_dwell_shape_size = 12.0;
        s.sampling_interval_ms = 1000;
        MovementClassifier::new(&s)
    }

    #[test]
    fn stationary_dwell_duration_increases_with_repeated_samples() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 150, 0.0, 0.0));
        let a = c.current_dwell_duration();
        c.accept_sample(sample(t0, 250, 0.0, 0.0));
        assert!(c.current_dwell_duration() > a);
    }
    #[test]
    fn dwell_does_not_appear_before_activation_delay() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 99, 0.0, 0.0));
        assert!(c.active_dwell().is_none());
    }
    #[test]
    fn jitter_inside_tolerance_does_not_move_center() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 10.0, 10.0));
        c.accept_sample(sample(t0, 150, 12.0, 11.0));
        let d = c.active_dwell().unwrap();
        assert_eq!((d.center_x, d.center_y), (10.0, 10.0));
    }
    #[test]
    fn exiting_tolerance_finalizes_exactly_one_dwell() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 150, 0.0, 0.0));
        c.accept_sample(sample(t0, 200, 10.0, 0.0));
        c.accept_sample(sample(t0, 250, 20.0, 0.0));
        assert_eq!(c.dwells.len(), 1);
    }
    #[test]
    fn later_dwell_at_same_position_is_independent() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 150, 0.0, 0.0));
        c.accept_sample(sample(t0, 200, 10.0, 0.0));
        c.accept_sample(sample(t0, 250, 0.0, 0.0));
        c.accept_sample(sample(t0, 400, 0.0, 0.0));
        c.accept_sample(sample(t0, 550, 0.0, 0.0));
        c.finalize_dwell();
        assert_eq!(c.dwells.len(), 2);
        assert_ne!(c.dwells[0].start_time, c.dwells[1].start_time);
    }
    #[test]
    fn maximum_dwell_size_clamps() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 1000, 0.0, 0.0));
        assert_eq!(c.active_dwell().unwrap().size, 12.0);
    }
    #[test]
    fn visual_setting_changes_finalize_active_geometry() {
        let t0 = Instant::now();
        let mut c = classifier();
        c.accept_sample(sample(t0, 0, 0.0, 0.0));
        c.accept_sample(sample(t0, 150, 0.0, 0.0));
        c.set_foreground_context(
            ApplicationIdentity::new(2, "other.exe", None, None),
            RgbaColor::new(9, 9, 9, 255),
        );
        assert_eq!(c.dwells.len(), 1);
    }
}
