use std::{
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    time::{Duration, Instant},
};

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
}
