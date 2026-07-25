use super::{
    controller::{DiscontinuityReason, MovementClassifier},
    error::EngineError,
    events::{EngineCommand, ResolvedDisplayProfile},
    model::RecordingStatus,
    snapshot::{EngineActivity, SessionSnapshot, SnapshotDeduper, TileDelta},
    statistics::SessionStatistics,
};
use crate::{
    canvas::{
        model::{CanvasModel, DwellShape, MovementPath},
        rasterizer::{rasterize_dwell_shape, rasterize_movement_path},
    },
    capture::{
        foreground::{ForegroundApplication, ForegroundResolver},
        sampler::{CursorSample, CursorSampler, ProductionSamplerFactory, SamplerFactory},
    },
    settings::model::AppSettings,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const ENGINE_COMMAND_BOUND: usize = 128;
pub const ENGINE_SNAPSHOT_BOUND: usize = 8;
const SAMPLE_BATCH: usize = 32;
type MonotonicNow = Box<dyn FnMut() -> Instant + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    QueueFull,
    Disconnected,
    ShuttingDown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownResult {
    Joined,
    AlreadyStopped,
}

pub struct RecordingEngineHandle {
    high_tx: SyncSender<EngineCommand>,
    normal_tx: SyncSender<EngineCommand>,
    pub snapshot_rx: Receiver<SessionSnapshot>,
    worker: Option<JoinHandle<()>>,
    shutting_down: bool,
}
impl RecordingEngineHandle {
    pub fn spawn(settings: AppSettings, foreground: Option<Box<dyn ForegroundResolver>>) -> Self {
        Self::spawn_with_factory(settings, foreground, Arc::new(ProductionSamplerFactory))
    }
    pub fn spawn_with_factory(
        settings: AppSettings,
        foreground: Option<Box<dyn ForegroundResolver>>,
        factory: Arc<dyn SamplerFactory>,
    ) -> Self {
        let (high_tx, high_rx) = sync_channel(32);
        let (normal_tx, normal_rx) = sync_channel(ENGINE_COMMAND_BOUND);
        let (snapshot_tx, snapshot_rx) = sync_channel(ENGINE_SNAPSHOT_BOUND);
        let worker = thread::spawn(move || {
            RecordingEngine::new_with_factory(settings, foreground, factory).run(
                high_rx,
                normal_rx,
                snapshot_tx,
            )
        });
        Self {
            high_tx,
            normal_tx,
            snapshot_rx,
            worker: Some(worker),
            shutting_down: false,
        }
    }
    pub fn try_submit(&self, cmd: EngineCommand) -> Result<(), SubmitError> {
        if self.shutting_down && !matches!(cmd, EngineCommand::ForceShutdown) {
            return Err(SubmitError::ShuttingDown);
        };
        let tx = if cmd.is_high_priority() {
            &self.high_tx
        } else {
            &self.normal_tx
        };
        tx.try_send(cmd).map_err(|e| match e {
            TrySendError::Full(_) => SubmitError::QueueFull,
            TrySendError::Disconnected(_) => SubmitError::Disconnected,
        })
    }
    pub fn orderly_shutdown(&mut self) -> ShutdownResult {
        if self.worker.is_none() {
            return ShutdownResult::AlreadyStopped;
        };
        self.shutting_down = true;
        let _ = self.high_tx.send(EngineCommand::PrepareShutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        ShutdownResult::Joined
    }
    pub fn force_shutdown(&mut self) -> ShutdownResult {
        if self.worker.is_none() {
            return ShutdownResult::AlreadyStopped;
        };
        self.shutting_down = true;
        let _ = self.high_tx.try_send(EngineCommand::ForceShutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        ShutdownResult::Joined
    }
    pub fn shutdown(&mut self) {
        let _ = self.orderly_shutdown();
    }
}
impl Drop for RecordingEngineHandle {
    fn drop(&mut self) {
        self.shutting_down = true;
        let _ = self.high_tx.try_send(EngineCommand::ForceShutdown); /* dropping JoinHandle detaches: UI destruction is bounded */
        self.worker.take();
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
    monotonic_now: MonotonicNow,
    status_messages: Vec<String>,
    errors: Vec<EngineError>,
    sequence: u64,
    generation: u64,
    deduper: SnapshotDeduper,
    sampler_factory: Arc<dyn SamplerFactory>,
    sampler: Option<Box<dyn CursorSampler>>,
    sample_rx: Option<Receiver<CursorSample>>,
    force_discontinuity: bool,
    ui_visible: bool,
    last_publish: Option<Instant>,
    sent_revisions: HashMap<crate::canvas::coordinates::TileCoordinate, u64>,
    removed: HashSet<crate::canvas::coordinates::TileCoordinate>,
    full_snapshot: bool,
    shutting_down: bool,
    detected_topology: crate::canvas::topology::DisplayTopology,
    profile: Option<crate::display_profiles::ImmutableDisplayProfileSnapshot>,
    excluded: bool,
}
impl RecordingEngine {
    pub fn new(settings: AppSettings, fg: Option<Box<dyn ForegroundResolver>>) -> Self {
        Self::new_with_factory(settings, fg, Arc::new(ProductionSamplerFactory))
    }
    pub fn new_with_factory(
        settings: AppSettings,
        fg: Option<Box<dyn ForegroundResolver>>,
        factory: Arc<dyn SamplerFactory>,
    ) -> Self {
        Self::new_parts(settings, fg, factory, Box::new(Instant::now))
    }
    pub fn new_with_clock(
        settings: AppSettings,
        fg: Option<Box<dyn ForegroundResolver>>,
        clock: MonotonicNow,
    ) -> Self {
        Self::new_parts(settings, fg, Arc::new(ProductionSamplerFactory), clock)
    }
    fn new_parts(
        settings: AppSettings,
        fg: Option<Box<dyn ForegroundResolver>>,
        factory: Arc<dyn SamplerFactory>,
        clock: MonotonicNow,
    ) -> Self {
        Self {
            status: RecordingStatus::Stopped,
            classifier: MovementClassifier::new(&settings),
            settings,
            canvas: Default::default(),
            statistics: Default::default(),
            foreground_resolver: fg,
            current_foreground: ForegroundApplication::unknown(),
            last_foreground_check: None,
            monotonic_now: clock,
            status_messages: vec![],
            errors: vec![],
            sequence: 0,
            generation: 1,
            deduper: Default::default(),
            sampler_factory: factory,
            sampler: None,
            sample_rx: None,
            force_discontinuity: false,
            ui_visible: true,
            last_publish: None,
            sent_revisions: HashMap::new(),
            removed: HashSet::new(),
            full_snapshot: true,
            shutting_down: false,
            detected_topology: Default::default(),
            profile: None,
            excluded: false,
        }
    }
    fn run(
        &mut self,
        high: Receiver<EngineCommand>,
        normal: Receiver<EngineCommand>,
        snapshots: SyncSender<SessionSnapshot>,
    ) {
        loop {
            while let Ok(c) = high.try_recv() {
                if self.apply(c) {
                    self.publish(&snapshots, true);
                    return;
                }
            }
            if let Ok(c) = normal.try_recv() {
                if self.apply(c) {
                    self.publish(&snapshots, true);
                    return;
                }
            };
            let mut handled = 0;
            while handled < SAMPLE_BATCH {
                while let Ok(c) = high.try_recv() {
                    if self.apply(c) {
                        self.publish(&snapshots, true);
                        return;
                    }
                }
                let next = self.sample_rx.as_ref().and_then(|r| r.try_recv().ok());
                match next {
                    Some(s) => {
                        if self.status == RecordingStatus::Recording {
                            self.accept_sample(s);
                            self.commit_finished();
                        }
                        handled += 1
                    }
                    None => break,
                }
            }
            self.publish(&snapshots, false);
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn start_sampler(&mut self) {
        self.stop_sampler();
        let mut s = self
            .sampler_factory
            .create(self.settings.sampling_interval_ms);
        self.sample_rx = Some(s.start());
        self.sampler = Some(s)
    }
    fn stop_sampler(&mut self) {
        self.sample_rx = None;
        if let Some(mut s) = self.sampler.take() {
            s.stop();
        }
    }
    fn apply(&mut self, cmd: EngineCommand) -> bool {
        if self.shutting_down
            && !matches!(
                cmd,
                EngineCommand::ForceShutdown
                    | EngineCommand::PrepareShutdown
                    | EngineCommand::RequestSnapshot
            )
        {
            self.status_messages
                .push("Command rejected: engine shutdown is in progress.".into());
            return false;
        }
        match cmd {
            EngineCommand::Start(ResolvedDisplayProfile {
                settings,
                detected_topology,
                effective_topology,
                profile,
            }) => {
                self.generation += 1;
                self.settings = (*settings).clone();
                self.detected_topology = detected_topology;
                self.canvas.current_topology = effective_topology;
                self.profile = Some(profile);
                self.excluded = false;
                self.classifier = MovementClassifier::new(&self.settings);
                self.status = RecordingStatus::Recording;
                self.full_snapshot = true;
                self.start_sampler();
                self.status_messages.push("Recording started.".into())
            }
            EngineCommand::Pause => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.stop_sampler();
                self.status = RecordingStatus::Paused
            }
            EngineCommand::Resume => {
                self.flush_all(DiscontinuityReason::PauseResume);
                self.force_discontinuity = true;
                self.start_sampler();
                self.status = RecordingStatus::Recording
            }
            EngineCommand::Finish => {
                self.stop_sampler();
                self.flush_all(DiscontinuityReason::PauseResume);
                self.status = RecordingStatus::Stopped
            }
            EngineCommand::Clear => {
                self.removed
                    .extend(self.canvas.sparse_tiles.tiles.keys().copied());
                self.canvas.clear();
                self.statistics.reset();
                self.generation += 1;
                self.sent_revisions.clear();
                self.full_snapshot = true
            }
            EngineCommand::UpdateRecordingParameters(s) => {
                let restart = self.status == RecordingStatus::Recording
                    && s.sampling_interval_ms != self.settings.sampling_interval_ms;
                self.flush_all(DiscontinuityReason::PauseResume);
                self.settings = s;
                self.classifier.update_settings(&self.settings);
                if restart {
                    self.force_discontinuity = true;
                    self.start_sampler()
                }
            }
            EngineCommand::UpdateDrawingStyle(s) | EngineCommand::UpdateBackground(s) => {
                self.settings = s
            }
            EngineCommand::UpdateApplicationColorRules(r) => self.settings.application_colors = r,
            EngineCommand::RefreshTopology(Some(t)) => {
                if t.signature != self.canvas.current_topology.signature {
                    self.flush_all(DiscontinuityReason::DisplayConfigurationChanged);
                    self.canvas.current_topology = t;
                    self.force_discontinuity = true
                }
            }
            EngineCommand::RefreshTopology(None) | EngineCommand::InvalidateTopology => {}
            EngineCommand::SetUiVisibility(v) => self.ui_visible = v,
            EngineCommand::RequestSnapshot => {
                self.full_snapshot = true;
                self.deduper.clear()
            }
            EngineCommand::RestoreStoppedSession(_) => {
                self.stop_sampler();
                self.status = RecordingStatus::Stopped;
                self.generation += 1;
                self.full_snapshot = true
            }
            EngineCommand::RequestExport(_) => {}
            EngineCommand::RequestRecoveryCheckpoint => {}
            EngineCommand::PrepareShutdown | EngineCommand::ForceShutdown => {
                self.shutting_down = true;
                self.stop_sampler();
                self.flush_all(DiscontinuityReason::PauseResume);
                return true;
            }
        }
        false
    }
    fn accept_sample(&mut self, s: CursorSample) {
        self.statistics.observed_samples += 1;
        let point = crate::canvas::coordinates::DesktopPoint::new(s.physical_x, s.physical_y);
        if self.detected_topology.monitor_containing(point).is_some()
            && self
                .canvas
                .current_topology
                .monitor_containing(point)
                .is_none()
        {
            if !self.excluded {
                self.flush_all(DiscontinuityReason::DisplayConfigurationChanged);
                self.excluded = true;
            }
            return;
        }
        if self.excluded {
            self.classifier
                .mark_discontinuity(DiscontinuityReason::DisplayConfigurationChanged);
            self.excluded = false;
        }
        self.statistics.samples_recorded += 1;
        if self.force_discontinuity {
            self.classifier
                .mark_discontinuity(DiscontinuityReason::PauseResume);
            self.force_discontinuity = false
        }
        self.resolve_foreground_bounded();
        let color = self.settings.application_colors.color_for(
            &self.current_foreground.identity,
            &self.settings.default_movement_color,
        );
        self.classifier
            .set_foreground_context(self.current_foreground.identity.clone(), color);
        self.classifier.accept_sample(s);
        self.statistics.current_dwell_duration = self.classifier.current_dwell_duration()
    }
    fn resolve_foreground_bounded(&mut self) {
        let now = (self.monotonic_now)();
        if self
            .last_foreground_check
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250))
        {
            return;
        }
        self.last_foreground_check = Some(now);
        let result = match self.foreground_resolver.as_mut() {
            Some(r) => r.resolve_foreground(),
            None => crate::capture::windows::resolve_foreground_application(),
        };
        match result {
            Ok(a) => self.current_foreground = a,
            Err(_) => {
                self.current_foreground = ForegroundApplication::unknown();
                if !self
                    .errors
                    .iter()
                    .any(|e| matches!(e, EngineError::ForegroundDegradation(_)))
                {
                    self.errors.push(EngineError::ForegroundDegradation(
                        "application identity unavailable".into(),
                    ))
                }
            }
        }
    }
    fn flush_all(&mut self, r: DiscontinuityReason) {
        self.classifier.mark_discontinuity(r);
        self.commit_finished()
    }
    fn commit_finished(&mut self) {
        for seg in self.classifier.segments.drain(..) {
            let mut p = MovementPath::new(seg.color, self.settings.line_width_px, true);
            p.application = seg.application;
            p.points = seg
                .points
                .into_iter()
                .map(|(x, y)| crate::canvas::coordinates::CanvasPoint { x, y })
                .collect();
            rasterize_movement_path(&mut self.canvas.sparse_tiles, &p);
            self.statistics.finalized_movement_chunks += 1
        }
        for d in self.classifier.dwells.drain(..) {
            let mut s = DwellShape::from_duration(
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
            self.statistics.finalized_dwells += 1
        }
        self.statistics.active_tile_count = self.canvas.sparse_tiles.tiles.len()
    }
    fn publish(&mut self, tx: &SyncSender<SessionSnapshot>, immediate: bool) {
        let now = (self.monotonic_now)();
        let cadence = if self.status == RecordingStatus::Recording {
            if self.ui_visible {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(1)
            }
        } else {
            Duration::MAX
        };
        let changed = self.full_snapshot
            || self
                .canvas
                .sparse_tiles
                .tiles
                .iter()
                .any(|(c, t)| self.sent_revisions.get(c) != Some(&t.revision))
            || !self.removed.is_empty();
        if !immediate && !changed && self.status == RecordingStatus::Stopped {
            return;
        }
        if !immediate
            && self
                .last_publish
                .is_some_and(|p| now.saturating_duration_since(p) < cadence)
        {
            return;
        }
        self.sequence += 1;
        let deltas = self
            .canvas
            .sparse_tiles
            .tiles
            .iter()
            .filter(|(c, t)| self.full_snapshot || self.sent_revisions.get(c) != Some(&t.revision))
            .map(|(c, t)| TileDelta {
                coordinate: *c,
                revision: t.revision,
                width: self.canvas.sparse_tiles.tile_size,
                height: self.canvas.sparse_tiles.tile_size,
                rgba: Arc::from(t.pixels.clone()),
                removed: false,
                generation: self.generation,
            })
            .chain(self.removed.iter().map(|c| TileDelta {
                coordinate: *c,
                revision: 0,
                width: 0,
                height: 0,
                rgba: Arc::from([]),
                removed: true,
                generation: self.generation,
            }))
            .collect::<Vec<_>>();
        let revisions = deltas
            .iter()
            .filter(|d| !d.removed)
            .map(|d| (d.coordinate, d.revision))
            .collect();
        let snap = SessionSnapshot {
            recording_status: self.status,
            session_id: None,
            detected_topology: self.detected_topology.clone(),
            effective_topology: self.canvas.current_topology.clone(),
            session_bounds: self.canvas.session_desktop_bounds,
            profile: self.profile.clone(),
            tile_deltas: deltas,
            full_tile_snapshot: self.full_snapshot,
            active_path_overlay: self.canvas.active_movement_overlay.clone(),
            active_dwell_overlay: self.canvas.active_dwell_overlay.clone(),
            changed_tile_revisions: revisions,
            current_topology: self.canvas.current_topology.clone(),
            session_topology: self.canvas.current_topology.clone(),
            statistics: self.statistics.clone(),
            sampler_observed: self.statistics.observed_samples,
            classifier_delivered: self.statistics.samples_recorded,
            samples_coalesced: 0,
            activity: EngineActivity::default(),
            status_messages: self.status_messages.clone(),
            errors: self.errors.clone(),
            sequence: self.sequence,
            generation: self.generation,
        };
        if self.deduper.should_send(&snap) && tx.try_send(snap).is_ok() {
            for (c, t) in &self.canvas.sparse_tiles.tiles {
                self.sent_revisions.insert(*c, t.revision);
            }
            self.removed.clear();
            self.full_snapshot = false;
            self.last_publish = Some(now)
        }
    }
}
