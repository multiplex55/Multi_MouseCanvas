use crate::app::state::AppState;
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub struct PerformanceDiagnostics {
    pub last_update: Instant,
    pub process_cpu_percent: f32,
    pub process_memory_bytes: u64,
    pub allocated_tile_count: usize,
    pub active_geometry_count: usize,
    pub dirty_tile_count: usize,
    pub samples_processed: u64,
}
impl Default for PerformanceDiagnostics {
    fn default() -> Self {
        Self {
            last_update: Instant::now() - Duration::from_secs(2),
            process_cpu_percent: 0.0,
            process_memory_bytes: 0,
            allocated_tile_count: 0,
            active_geometry_count: 0,
            dirty_tile_count: 0,
            samples_processed: 0,
        }
    }
}
impl PerformanceDiagnostics {
    pub fn refresh_if_due(&mut self, state: &AppState, now: Instant) -> bool {
        if now.duration_since(self.last_update) < Duration::from_secs(1) {
            return false;
        }
        self.last_update = now;
        self.allocated_tile_count = state.canvas.sparse_tiles.touched_tile_count();
        self.dirty_tile_count = state
            .canvas
            .sparse_tiles
            .tiles
            .values()
            .filter(|t| t.preview_dirty)
            .count();
        self.active_geometry_count = usize::from(state.canvas.active_movement_overlay.is_some())
            + usize::from(state.canvas.active_dwell_overlay.is_some());
        self.samples_processed = state.statistics.samples_recorded;
        self.process_memory_bytes = current_process_memory_bytes();
        true
    }
}
#[cfg(windows)]
fn current_process_memory_bytes() -> u64 {
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS::default();
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .as_bool()
        {
            c.WorkingSetSize as u64
        } else {
            0
        }
    }
}
#[cfg(not(windows))]
fn current_process_memory_bytes() -> u64 {
    0
}
pub fn show(ui: &mut eframe::egui::Ui, state: &mut AppState) {
    let mut d = std::mem::take(&mut state.performance_diagnostics);
    d.refresh_if_due(state, Instant::now());
    ui.label(format!("CPU: {:.1}%", d.process_cpu_percent));
    ui.label(format!(
        "Memory: {:.1} MiB",
        d.process_memory_bytes as f64 / 1048576.0
    ));
    ui.label(format!("Allocated tiles: {}", d.allocated_tile_count));
    ui.label(format!("Active geometry: {}", d.active_geometry_count));
    ui.label(format!("Dirty tiles: {}", d.dirty_tile_count));
    ui.label(format!("Samples processed: {}", d.samples_processed));
    state.performance_diagnostics = d;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn performance_diagnostics_cadence_is_approximately_one_second() {
        let state = AppState::default();
        let base = Instant::now();
        let mut d = PerformanceDiagnostics {
            last_update: base,
            ..Default::default()
        };
        assert!(!d.refresh_if_due(&state, base + Duration::from_millis(999)));
        assert!(d.refresh_if_due(&state, base + Duration::from_secs(1)));
    }
}
