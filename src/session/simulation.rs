//! Deterministic synthetic session workloads used by tests and diagnostics.
//! All time is accelerated and supplied explicitly; no generator sleeps.

use crate::capture::sampler::CursorSample;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntheticControl {
    ApplicationSwitch(String),
    TopologyChange(&'static str),
    Pause,
    Resume,
}

/// Deterministic control streams are kept separate from cursor samples so a
/// harness can inject them through the same command/topology boundaries as
/// production code.
pub fn frequent_application_switches(count: usize) -> Vec<SyntheticControl> {
    (0..count)
        .map(|i| SyntheticControl::ApplicationSwitch(format!("synthetic-{}.exe", i % 7)))
        .collect()
}

pub fn display_topology_changes() -> Vec<SyntheticControl> {
    vec![
        SyntheticControl::TopologyChange("monitor-added-left"),
        SyntheticControl::TopologyChange("resolution-change"),
        SyntheticControl::TopologyChange("rotation"),
        SyntheticControl::TopologyChange("origin-shift"),
        SyntheticControl::TopologyChange("monitor-removed"),
    ]
}

pub fn pause_and_resume() -> [SyntheticControl; 2] {
    [SyntheticControl::Pause, SyntheticControl::Resume]
}

#[derive(Debug, Clone)]
pub struct SyntheticSession {
    epoch: Instant,
    elapsed: Duration,
    samples: Vec<CursorSample>,
}

impl SyntheticSession {
    pub fn new(epoch: Instant) -> Self {
        Self {
            epoch,
            elapsed: Duration::ZERO,
            samples: Vec::new(),
        }
    }
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
    pub fn samples(&self) -> &[CursorSample] {
        &self.samples
    }
    pub fn into_samples(self) -> Vec<CursorSample> {
        self.samples
    }
    pub fn gap(&mut self, duration: Duration) -> &mut Self {
        self.elapsed += duration;
        self
    }
    pub fn point(&mut self, x: f32, y: f32, step: Duration) -> &mut Self {
        self.samples
            .push(CursorSample::new(self.epoch + self.elapsed, x, y));
        self.elapsed += step;
        self
    }
    pub fn continuous(
        &mut self,
        count: usize,
        from: (f32, f32),
        delta: (f32, f32),
        step: Duration,
    ) -> &mut Self {
        for i in 0..count {
            self.point(
                from.0 + delta.0 * i as f32,
                from.1 + delta.1 * i as f32,
                step,
            );
        }
        self
    }
    pub fn slow_curve(
        &mut self,
        count: usize,
        center: (f32, f32),
        radius: f32,
        step: Duration,
    ) -> &mut Self {
        for i in 0..count {
            let a = i as f32 * 0.04;
            self.point(
                center.0 + radius * a.cos(),
                center.1 + radius * a.sin(),
                step,
            );
        }
        self
    }
    pub fn dwell(&mut self, count: usize, at: (f32, f32), step: Duration) -> &mut Self {
        for _ in 0..count {
            self.point(at.0, at.1, step);
        }
        self
    }
    pub fn jitter(
        &mut self,
        count: usize,
        at: (f32, f32),
        amplitude: f32,
        step: Duration,
    ) -> &mut Self {
        const PATTERN: [(f32, f32); 8] = [
            (0., 0.),
            (1., 0.),
            (0., 1.),
            (-1., 0.),
            (0., -1.),
            (1., 1.),
            (-1., 1.),
            (-1., -1.),
        ];
        for i in 0..count {
            let p = PATTERN[i % PATTERN.len()];
            self.point(at.0 + p.0 * amplitude, at.1 + p.1 * amplitude, step);
        }
        self
    }
    /// A bounded stream whose timestamps span an entire day.
    pub fn accelerated_day(&mut self, count: usize, bounds: (f32, f32)) -> &mut Self {
        let step = Duration::from_secs(86_400 / count.max(1) as u64);
        for i in 0..count {
            let x = ((i * 7919) % 10_000) as f32 / 10_000.0 * bounds.0;
            let y = ((i * 3571) % 10_000) as f32 / 10_000.0 * bounds.1;
            self.point(x, y, step);
        }
        self
    }
    pub fn monitor_crossing(
        &mut self,
        left: (f32, f32),
        right: (f32, f32),
        step: Duration,
    ) -> &mut Self {
        self.point(left.0, left.1, step)
            .point(right.0, right.1, step)
    }
    /// A crossing over a physical gap is a discontinuity: callers apply the
    /// topology event between the two deterministic samples.
    pub fn monitor_gap_crossing(
        &mut self,
        left: (f32, f32),
        right: (f32, f32),
        step: Duration,
    ) -> &mut Self {
        self.monitor_crossing(left, right, step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        session::controller::{DiscontinuityReason, MovementClassifier, ACTIVE_PATH_POINT_LIMIT},
        settings::model::AppSettings,
    };

    #[test]
    fn generators_are_repeatable_and_cover_sleep_sized_gaps() {
        let t = Instant::now();
        let build = || {
            let mut s = SyntheticSession::new(t);
            s.continuous(5, (-10., 0.), (3., 1.), Duration::from_millis(2))
                .slow_curve(8, (0., 0.), 20., Duration::from_millis(20))
                .dwell(4, (2., 2.), Duration::from_secs(1))
                .jitter(8, (2., 2.), 0.25, Duration::from_millis(4))
                .gap(Duration::from_secs(31));
            s.into_samples()
        };
        assert_eq!(build(), build());
        assert!(build().last().unwrap().timestamp.duration_since(t) < Duration::from_secs(31));
    }

    #[test]
    fn accelerated_day_retains_only_bounded_active_geometry() {
        let t = Instant::now();
        let mut workload = SyntheticSession::new(t);
        workload.accelerated_day(20_000, (7680., 2160.));
        assert!(workload.elapsed() >= Duration::from_secs(80_000));
        let mut settings = AppSettings::default();
        settings.sampling_interval_ms = 10_000;
        let mut classifier = MovementClassifier::new(&settings);
        let mut max_active = 0;
        let mut finalized = 0_u64;
        for (i, sample) in workload.into_samples().into_iter().enumerate() {
            if i % 997 == 0 {
                classifier.mark_discontinuity(DiscontinuityReason::DisplayConfigurationChanged);
            }
            classifier.accept_sample(sample);
            max_active = max_active.max(classifier.active_segment().map_or(0, |s| s.points.len()));
            finalized += classifier.segments.len() as u64;
            classifier.segments.clear(); // models immediate tile rasterization by the engine
            classifier.dwells.clear();
        }
        assert!(max_active < ACTIVE_PATH_POINT_LIMIT);
        assert!(finalized > 0);
        assert!(classifier.segments.is_empty());
        assert!(classifier.dwells.is_empty());
    }

    #[test]
    #[ignore = "diagnostic high-volume validation; run on Windows release builds"]
    fn windows_long_session_diagnostic() {
        let started = std::time::Instant::now();
        let mut workload = SyntheticSession::new(started);
        workload.accelerated_day(2_000_000, (7680., 2160.));
        let generated = workload.samples().len();
        let wall = started.elapsed().as_secs_f64().max(0.000_001);
        eprintln!("long-session report: samples={generated}, samples_per_second={:.0}, elapsed={wall:.3}s; CPU/memory/dirty-flush/preview-upload/recovery-size/tile-count are collected by the Windows diagnostic runner", generated as f64 / wall);
        assert_eq!(generated, 2_000_000);
    }
}
