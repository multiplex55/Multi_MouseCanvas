use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SampleMailboxCounters {
    pub observed: u64,
    pub delivered: u64,
    pub coalesced: u64,
}

#[derive(Debug)]
struct MailboxState {
    fifo: VecDeque<CursorSample>,
    latest: Option<CursorSample>,
    closed: bool,
    counters: SampleMailboxCounters,
}

/// A bounded ordered queue with one replaceable overflow slot. Producers never
/// block; replacing the slot notifies the consumer immediately.
#[derive(Debug, Clone)]
pub struct SampleMailbox {
    inner: Arc<(Mutex<MailboxState>, Condvar)>,
    capacity: usize,
}
impl SampleMailbox {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            inner: Arc::new((
                Mutex::new(MailboxState {
                    fifo: VecDeque::with_capacity(capacity),
                    latest: None,
                    closed: false,
                    counters: Default::default(),
                }),
                Condvar::new(),
            )),
            capacity,
        }
    }
    pub fn push(&self, sample: CursorSample) -> Result<(), CursorSample> {
        let (lock, wake) = &*self.inner;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        if s.closed {
            return Err(sample);
        }
        s.counters.observed += 1;
        if s.fifo.len() < self.capacity {
            s.fifo.push_back(sample);
        } else {
            s.latest = Some(sample);
            s.counters.coalesced += 1;
        }
        wake.notify_one();
        Ok(())
    }
    pub fn try_pop(&self) -> Option<CursorSample> {
        let mut s = self.inner.0.lock().unwrap_or_else(|e| e.into_inner());
        let value = s.fifo.pop_front().or_else(|| s.latest.take());
        if value.is_some() {
            s.counters.delivered += 1;
        }
        value
    }
    pub fn pop_timeout(&self, timeout: Duration) -> Option<CursorSample> {
        let (lock, wake) = &*self.inner;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        if s.fifo.is_empty() && s.latest.is_none() && !s.closed {
            s = wake
                .wait_timeout(s, timeout)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let value = s.fifo.pop_front().or_else(|| s.latest.take());
        if value.is_some() {
            s.counters.delivered += 1;
        }
        value
    }
    pub fn counters(&self) -> SampleMailboxCounters {
        self.inner
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .counters
    }
    pub fn stored_len(&self) -> usize {
        let s = self.inner.0.lock().unwrap_or_else(|e| e.into_inner());
        s.fifo.len() + usize::from(s.latest.is_some())
    }
    pub fn close(&self) {
        let (l, w) = &*self.inner;
        l.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        w.notify_all();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorSample {
    pub timestamp: Instant,
    pub physical_x: f32,
    pub physical_y: f32,
    pub foreground_application: Option<String>,
    pub monitor_identity: Option<String>,
}

impl CursorSample {
    pub fn new(timestamp: Instant, physical_x: f32, physical_y: f32) -> Self {
        Self {
            timestamp,
            physical_x,
            physical_y,
            foreground_application: None,
            monitor_identity: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerCommand {
    Stop,
}

pub const SAMPLE_CHANNEL_BOUND: usize = 256;

pub fn bounded_sample_channel() -> (SyncSender<CursorSample>, Receiver<CursorSample>) {
    mpsc::sync_channel(SAMPLE_CHANNEL_BOUND)
}

pub fn try_send_coalesced(tx: &SyncSender<CursorSample>, sample: CursorSample) -> bool {
    match tx.try_send(sample) {
        Ok(()) => true,
        Err(mpsc::TrySendError::Full(_)) => false,
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

pub trait CursorSampler: Send {
    fn start(&mut self) -> Receiver<CursorSample>;
    fn stop(&mut self);
}

pub trait SamplerFactory: Send + Sync {
    fn create(&self, sampling_interval_ms: u64) -> Box<dyn CursorSampler>;
}

#[derive(Debug, Default)]
pub struct ProductionSamplerFactory;
impl SamplerFactory for ProductionSamplerFactory {
    fn create(&self, sampling_interval_ms: u64) -> Box<dyn CursorSampler> {
        Box::new(crate::capture::windows::WindowsPollingSampler::new(
            sampling_interval_ms,
        ))
    }
}

#[derive(Debug)]
pub struct FakeCursorSampler {
    samples: Vec<CursorSample>,
    command_tx: Option<Sender<SamplerCommand>>,
}

impl FakeCursorSampler {
    pub fn new(samples: Vec<CursorSample>) -> Self {
        Self {
            samples,
            command_tx: None,
        }
    }
}

impl CursorSampler for FakeCursorSampler {
    fn start(&mut self) -> Receiver<CursorSample> {
        let (sample_tx, sample_rx) = bounded_sample_channel();
        let (command_tx, command_rx) = mpsc::channel();
        self.command_tx = Some(command_tx);
        for sample in self.samples.clone() {
            if command_rx.try_recv().is_ok() {
                break;
            }
            let _ = try_send_coalesced(&sample_tx, sample);
        }
        sample_rx
    }

    fn stop(&mut self) {
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(SamplerCommand::Stop);
        }
    }
}

pub fn sampling_interval_from_ms(ms: u64) -> Duration {
    Duration::from_millis(ms.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_sampler_emits_configured_samples() {
        let now = Instant::now();
        let mut sampler = FakeCursorSampler::new(vec![CursorSample::new(now, 1.0, 2.0)]);
        let rx = sampler.start();
        let sample = rx.recv().expect("fake sample");
        assert_eq!(sample.physical_x, 1.0);
        assert_eq!(sample.physical_y, 2.0);
        sampler.stop();
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    #[test]
    fn bounded_sample_channel_does_not_grow_unbounded() {
        let (tx, _rx) = bounded_sample_channel();
        let now = Instant::now();
        let mut accepted = 0;
        for i in 0..(SAMPLE_CHANNEL_BOUND + 100) {
            if try_send_coalesced(&tx, CursorSample::new(now, i as f32, 0.0)) {
                accepted += 1;
            }
        }
        assert_eq!(accepted, SAMPLE_CHANNEL_BOUND);
    }
    #[test]
    fn mailbox_is_bounded_and_preserves_fifo_then_latest() {
        let m = SampleMailbox::new(2);
        let now = Instant::now();
        for x in 0..5 {
            m.push(CursorSample::new(now, x as f32, 0.)).unwrap();
        }
        assert_eq!(m.stored_len(), 3);
        assert_eq!(m.try_pop().unwrap().physical_x, 0.);
        assert_eq!(m.try_pop().unwrap().physical_x, 1.);
        assert_eq!(m.try_pop().unwrap().physical_x, 4.);
        assert_eq!(
            m.counters(),
            SampleMailboxCounters {
                observed: 5,
                delivered: 3,
                coalesced: 3
            }
        );
    }
}
