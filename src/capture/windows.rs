//! Windows-specific cursor polling sampler.

use super::sampler::{sampling_interval_from_ms, CursorSample, CursorSampler, SamplerCommand};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct WindowsPollingSampler {
    sampling_interval: Duration,
    command_tx: Option<Sender<SamplerCommand>>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsPollingSampler {
    pub fn new(sampling_interval_ms: u64) -> Self {
        Self {
            sampling_interval: sampling_interval_from_ms(sampling_interval_ms),
            command_tx: None,
            worker: None,
        }
    }
}

impl CursorSampler for WindowsPollingSampler {
    fn start(&mut self) -> Receiver<CursorSample> {
        self.stop();
        let (sample_tx, sample_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        let sampling_interval = self.sampling_interval;
        self.command_tx = Some(command_tx);
        self.worker = Some(thread::spawn(move || loop {
            if command_rx.try_recv().is_ok() {
                break;
            }
            if let Some((x, y)) = global_cursor_position() {
                let _ = sample_tx.send(CursorSample::new(Instant::now(), x as f32, y as f32));
            }
            thread::sleep(sampling_interval);
        }));
        sample_rx
    }

    fn stop(&mut self) {
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(SamplerCommand::Stop);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for WindowsPollingSampler {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
fn global_cursor_position() -> Option<(i32, i32)> {
    use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)
            .as_bool()
            .then_some((point.x, point.y))
    }
}

#[cfg(not(windows))]
fn global_cursor_position() -> Option<(i32, i32)> {
    None
}
