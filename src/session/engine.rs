use super::{
    controller::{DiscontinuityReason, MovementClassifier},
    events::EngineCommand,
    model::RecordingStatus,
    snapshot::{SessionSnapshot, SnapshotDeduper},
    statistics::SessionStatistics,
};
use crate::{
    canvas::{
        model::CanvasModel,
        rasterizer::{rasterize_dwell_shape, rasterize_movement_path},
    },
    capture::{
        foreground::{ForegroundApplication, ForegroundResolver},
        sampler::CursorSample,
    },
    settings::model::AppSettings,
};
use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const ENGINE_COMMAND_BOUND: usize = 128;
pub const ENGINE_SAMPLE_BOUND: usize = 256;
pub const ENGINE_SNAPSHOT_BOUND: usize = 8;

pub struct RecordingEngineHandle {
    pub command_tx: SyncSender<EngineCommand>,
    pub sample_tx: SyncSender<CursorSample>,
    pub snapshot_rx: Receiver<SessionSnapshot>,
    worker: Option<JoinHandle<()>>,
}
impl RecordingEngineHandle {
    pub fn spawn(settings: AppSettings, foreground: Option<Box<dyn ForegroundResolver>>) -> Self {
        let (command_tx, command_rx) = sync_channel(ENGINE_COMMAND_BOUND);
        let (sample_tx, sample_rx) = sync_channel(ENGINE_SAMPLE_BOUND);
        let (snapshot_tx, snapshot_rx) = sync_channel(ENGINE_SNAPSHOT_BOUND);
        let worker = thread::spawn(move || {
            let mut e = RecordingEngine::new(settings, foreground);
            e.run(command_rx, sample_rx, snapshot_tx);
        });
        Self {
            command_tx,
            sample_tx,
            snapshot_rx,
            worker: Some(worker),
        }
    }
    pub fn shutdown(&mut self) {
        let _ = self.command_tx.try_send(EngineCommand::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
impl Drop for RecordingEngineHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct RecordingEngine {
    status: RecordingStatus,
    settings: AppSettings,
    canvas: CanvasModel,
    classifier: MovementClassifier,
    statistics: SessionStatistics,
    foreground_resolver: Option<Box<dyn ForegroundResolver>>,
    current_foreground: ForegroundApplication,
    last_foreground_check: Option<Instant>,
    status_messages: Vec<String>,
    errors: Vec<String>,
    sequence: u64,
    deduper: SnapshotDeduper,
}
impl RecordingEngine {
    pub fn new(
        settings: AppSettings,
        foreground_resolver: Option<Box<dyn ForegroundResolver>>,
    ) -> Self {
        Self {
            status: RecordingStatus::Stopped,
            classifier: MovementClassifier::new(&settings),
            settings,
            canvas: CanvasModel::default(),
            statistics: SessionStatistics::default(),
            foreground_resolver,
            current_foreground: ForegroundApplication::unknown(),
            last_foreground_check: None,
            status_messages: Vec::new(),
            errors: Vec::new(),
            sequence: 0,
            deduper: SnapshotDeduper::default(),
        }
    }
    fn run(
        &mut self,
        commands: Receiver<EngineCommand>,
        samples: Receiver<CursorSample>,
        snapshots: SyncSender<SessionSnapshot>,
    ) {
        loop {
            while let Ok(cmd) = commands.try_recv() {
                if self.apply(cmd) {
                    return;
                }
            }
            match samples.recv_timeout(Duration::from_millis(50)) {
                Ok(sample) => {
                    while let Ok(cmd) = commands.try_recv() {
                        if self.apply(cmd) {
                            return;
                        }
                    }
                    if self.status == RecordingStatus::Recording {
                        self.accept_sample(sample);
                        self.commit_finished();
                        self.send_snapshot(&snapshots);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => self.send_snapshot(&snapshots),
                Err(_) => return,
            }
        }
    }
    fn apply(&mut self, cmd: EngineCommand) -> bool {
        match cmd {
            EngineCommand::Start => {
                self.status = RecordingStatus::Recording;
                self.status_messages.push("Recording started.".into());
            }
            EngineCommand::Pause => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.status = RecordingStatus::Paused;
            }
            EngineCommand::Resume => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.status = RecordingStatus::Recording;
            }
            EngineCommand::Finish => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.status = RecordingStatus::Stopped;
            }
            EngineCommand::Clear => {
                self.canvas.clear();
                self.statistics.reset();
                self.classifier = MovementClassifier::new(&self.settings);
                self.deduper.clear();
            }
            EngineCommand::UpdateDrawingSettings(s) => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.settings = s.clone();
                self.classifier = MovementClassifier::new(&s);
            }
            EngineCommand::UpdateApplicationColorRules(r) => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.settings.application_colors = r;
            }
            EngineCommand::RefreshTopology(t) => {
                self.flush_all(DiscontinuityReason::DisplayConfigurationChanged);
                if let Some(t) = t {
                    self.canvas.current_topology = t;
                }
            }
            EngineCommand::RequestSnapshot => {}
            EngineCommand::Shutdown => {
                self.flush_all(DiscontinuityReason::PauseResume);
                return true;
            }
        }
        false
    }
    fn accept_sample(&mut self, sample: CursorSample) {
        self.statistics.observed_samples += 1;
        self.statistics.samples_recorded += 1;
        self.resolve_foreground_bounded();
        let color = self.settings.application_colors.color_for(
            &self.current_foreground.identity,
            &self.settings.default_movement_color,
        );
        self.classifier
            .set_foreground_context(self.current_foreground.identity.clone(), color);
        self.classifier.accept_sample(sample);
        self.statistics.current_dwell_duration = self.classifier.current_dwell_duration();
    }
    fn resolve_foreground_bounded(&mut self) {
        if self
            .last_foreground_check
            .is_some_and(|t| t.elapsed() < Duration::from_millis(250))
        {
            return;
        }
        self.last_foreground_check = Some(Instant::now());
        self.current_foreground = match self.foreground_resolver.as_mut() {
            Some(r) => r
                .resolve_foreground()
                .unwrap_or_else(|_| ForegroundApplication::unknown()),
            None => crate::capture::windows::resolve_foreground_application()
                .unwrap_or_else(|_| ForegroundApplication::unknown()),
        };
    }
    fn flush_all(&mut self, reason: DiscontinuityReason) {
        self.classifier.mark_discontinuity(reason);
        self.commit_finished();
    }
    fn commit_finished(&mut self) {
        for seg in self.classifier.segments.drain(..) {
            let mut p = crate::canvas::model::MovementPath::new(
                seg.color,
                self.settings.line_width_px,
                true,
            );
            p.application = seg.application;
            p.points = seg
                .points
                .into_iter()
                .map(|(x, y)| crate::canvas::coordinates::CanvasPoint { x, y })
                .collect();
            rasterize_movement_path(&mut self.canvas.sparse_tiles, &p);
            self.statistics.finalized_movement_chunks += 1;
        }
        for d in self.classifier.dwells.drain(..) {
            let mut s = crate::canvas::model::DwellShape::from_duration(
                crate::canvas::coordinates::CanvasPoint {
                    x: d.center_x,
                    y: d.center_y,
                },
                d.duration,
                d.color,
                self.settings.selected_dwell_shape,
                self.settings.min_dwell_shape_size,
                self.settings.max_dwell_shape_size,
                self.settings.dwell_growth_rate,
                self.settings.dwell_fill_opacity,
                self.settings.dwell_outline_width,
                self.settings.dwell_render_mode,
                true,
            );
            s.application = d.application;
            rasterize_dwell_shape(&mut self.canvas.sparse_tiles, &s);
            self.statistics.finalized_dwells += 1;
            self.statistics.longest_dwell = self.statistics.longest_dwell.max(d.duration);
        }
        self.statistics.active_tile_count = self.canvas.sparse_tiles.tiles.len();
        self.statistics.dirty_tile_count = self
            .canvas
            .sparse_tiles
            .tiles
            .values()
            .filter(|t| t.preview_dirty || t.recovery_dirty)
            .count();
    }
    fn send_snapshot(&mut self, tx: &SyncSender<SessionSnapshot>) {
        self.sequence += 1;
        let snap = SessionSnapshot {
            recording_status: self.status,
            active_path_overlay: None,
            active_dwell_overlay: None,
            changed_tile_revisions: self
                .canvas
                .sparse_tiles
                .tiles
                .iter()
                .filter(|(_, t)| t.preview_dirty || t.recovery_dirty)
                .map(|(c, t)| (*c, t.revision))
                .collect(),
            current_topology: self.canvas.current_topology.clone(),
            session_topology: self.canvas.current_topology.clone(),
            statistics: self.statistics.clone(),
            status_messages: self.status_messages.clone(),
            errors: self.errors.clone(),
            sequence: self.sequence,
        };
        if self.deduper.should_send(&snap) {
            let _ = tx.try_send(snap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::foreground::{ApplicationIdentity, ForegroundError};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingResolver {
        calls: Arc<AtomicUsize>,
    }
    impl ForegroundResolver for CountingResolver {
        fn resolve_foreground(&mut self) -> Result<ForegroundApplication, ForegroundError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ForegroundApplication {
                identity: ApplicationIdentity::new(1, "app.exe", Some("c:/app.exe".into()), None),
                display_label: None,
            })
        }
    }

    #[test]
    fn foreground_metadata_is_not_resolved_for_every_sample() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut e = RecordingEngine::new(
            AppSettings::default(),
            Some(Box::new(CountingResolver {
                calls: calls.clone(),
            })),
        );
        e.status = RecordingStatus::Recording;
        let t0 = Instant::now();
        for i in 0..20 {
            e.accept_sample(CursorSample::new(
                t0 + Duration::from_millis(i),
                i as f32,
                0.0,
            ));
        }
        assert!(calls.load(Ordering::SeqCst) < 20);
    }
}
