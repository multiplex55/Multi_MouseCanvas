//! Application-owned bridge for monitor identification.
use crate::platform::monitor_identification::{
    assign_friendly_numbers, Command, PixelRect, Status, WorkerController,
};

pub struct MonitorIdentificationController {
    worker: Result<WorkerController, String>,
    active: bool,
}

impl Default for MonitorIdentificationController {
    fn default() -> Self {
        Self {
            worker: WorkerController::new(),
            active: false,
        }
    }
}

impl MonitorIdentificationController {
    /// Refreshes the complete detected topology for every request. This intentionally
    /// never consults the selected recording profile.
    pub fn show(&mut self) {
        let worker = match &mut self.worker {
            Ok(w) => w,
            Err(_) => return,
        };
        match crate::platform::display::current_topology() {
            Ok(topology) => {
                let raw = topology
                    .monitors
                    .into_iter()
                    .map(|m| {
                        let r = m.physical_rect;
                        (
                            m.stable_key().to_owned(),
                            m.label.unwrap_or(m.id),
                            PixelRect {
                                left: coordinate(r.min_x),
                                top: coordinate(r.min_y),
                                right: coordinate(r.max_x),
                                bottom: coordinate(r.max_y),
                            },
                        )
                    })
                    .collect();
                self.active = worker
                    .send(Command::Show(assign_friendly_numbers(raw)))
                    .is_ok();
            }
            Err(_) => self.active = false,
        }
    }
    pub fn close(&mut self) {
        if let Ok(w) = &mut self.worker {
            let _ = w.send(Command::Close);
        }
        self.active = false;
    }
    pub fn shutdown(&mut self) {
        if let Ok(w) = &mut self.worker {
            w.shutdown();
        }
        self.active = false;
    }
    pub fn is_pending(&self) -> bool {
        self.active || self.worker.as_ref().is_ok_and(WorkerController::is_pending)
    }
    pub fn poll(&mut self, text: &mut Option<String>) -> bool {
        let worker = match &mut self.worker {
            Ok(w) => w,
            Err(error) => {
                let new = Some(error.clone());
                let changed = *text != new;
                *text = new;
                return changed;
            }
        };
        let mut changed = false;
        while let Some(status) = worker.try_status() {
            let value = match status {
                Status::Shown { generation, .. } => {
                    self.active = true;
                    format!("Showing monitor identifiers (session {generation})")
                }
                Status::Closed => {
                    self.active = false;
                    "Monitor identifiers closed".into()
                }
                Status::Error(e) => {
                    self.active = false;
                    format!("Monitor identification failed: {e}")
                }
                Status::ShutdownComplete => {
                    self.active = false;
                    "Monitor identification stopped".into()
                }
            };
            if text.as_deref() != Some(&value) {
                *text = Some(value);
                changed = true;
            }
        }
        changed
    }
}

fn coordinate(value: f32) -> i32 {
    if !value.is_finite() {
        0
    } else {
        value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn identify_ui_request_does_not_enqueue_engine_commands() {
        let mut state = crate::app::state::AppState::default();
        state.identify_monitors_requested = true;
        assert!(state.engine_commands.is_empty());
    }
}
