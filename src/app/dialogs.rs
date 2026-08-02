use crate::app::{commands::NewSessionOutcome, state::AppState};
use eframe::egui;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearDialogAction {
    Cancel,
    ConfirmClear,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LifecycleDialogState {
    pub clear_confirmation_open: bool,
    pub new_session_open: bool,
}
impl LifecycleDialogState {
    pub fn request_clear(&mut self, non_empty: bool) {
        self.clear_confirmation_open = non_empty;
    }
    pub fn clear_transition(&mut self, a: ClearDialogAction) -> bool {
        match a {
            ClearDialogAction::Cancel => {
                self.clear_confirmation_open = false;
                false
            }
            ClearDialogAction::ConfirmClear => {
                self.clear_confirmation_open = false;
                true
            }
        }
    }
}
pub fn show(
    ctx: &egui::Context,
    state: &mut AppState,
    lifecycle: &mut crate::app::lifecycle::LifecycleCoordinator,
) {
    if state.export_busy {
        egui::Window::new("Exporting")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    "Compositing full session tiles on a worker thread. Recording remains active.",
                );
                ui.add(egui::ProgressBar::new(state.export_progress).animate(true));
            });
    }
    if state.pending_new_session_decision {
        egui::Window::new("Start new session?")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Choose what to do with the completed session.");
                if ui
                    .button("Preserve completed session and start new")
                    .clicked()
                {
                    state.queue(crate::session::events::EngineCommand::RequestRecoveryCheckpoint);
                    state.resolve_new_session(NewSessionOutcome::ClearPreviousCanvas);
                }
                if ui.button("Export and start new").clicked() {
                    state.export_and_start_new_session();
                }
                if ui.button("Clear and start new").clicked() {
                    state.resolve_new_session(NewSessionOutcome::ClearPreviousCanvas);
                }
                if ui.button("Cancel").clicked() {
                    state.resolve_new_session(NewSessionOutcome::Cancel);
                }
            });
    }
    if state.lifecycle_dialogs.clear_confirmation_open {
        egui::Window::new("Clear canvas?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Canvas artwork will be cleared.");
                ui.label("Current recovery will be deleted if applicable.");
                ui.label("Application color settings remain.");
                ui.horizontal(|ui| {
                    if ui.button("Clear canvas").clicked() {
                        state.confirm_clear_canvas();
                    }
                    if ui.button("Cancel").clicked() {
                        state.lifecycle_dialogs.clear_confirmation_open = false;
                    }
                });
            });
    }
    if lifecycle.confirmation_pending() {
        egui::Window::new("Exit MultiMouseCanvas?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Exit and stop any background recording/sampling?");
                ui.horizontal(|ui| {
                    if ui.button("Exit").clicked() {
                        lifecycle.confirm_exit(std::time::Instant::now());
                    }
                    if ui.button("Cancel").clicked() {
                        lifecycle.cancel_confirmation();
                    }
                });
            });
    }
    if matches!(
        lifecycle.state(),
        crate::app::lifecycle::LifecycleState::StartingShutdown
            | crate::app::lifecycle::LifecycleState::WaitingForEngineAndRecovery
    ) {
        egui::Window::new("Saving recovery and exiting…")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Finalizing activity and writing an incomplete recovery checkpoint.");
                ui.spinner();
                let now = std::time::Instant::now();
                ui.label(format!(
                    "Elapsed: {:.1}s",
                    lifecycle.elapsed(now).as_secs_f32()
                ));
                if lifecycle.force_visible(now) && ui.button("Force exit").clicked() {
                    tracing::warn!("user requested force exit");
                    lifecycle.force();
                }
            });
    }
    if matches!(
        lifecycle.state(),
        crate::app::lifecycle::LifecycleState::RecoveryFailedAwaitingChoice
    ) {
        egui::Window::new("Recovery could not be saved").collapsible(false).resizable(false).show(ctx,|ui| {
            if let Some(result)=&lifecycle.outcome { match result { crate::session::shutdown::ShutdownResult::RecoveryFailedWorkersStopped{error,previous_recovery_valid}|crate::session::shutdown::ShutdownResult::WorkerShutdownFailed{error,previous_recovery_valid}=>{ui.label(error);ui.label(if *previous_recovery_valid{"The previous valid recovery was preserved."}else{"The previous recovery could not be guaranteed valid."});},_=>{} } }
            if ui.button("Exit anyway").clicked() && !lifecycle.exit_anyway() { lifecycle.force(); }
            ui.label("Cancel exit is unavailable because the authoritative engine has already stopped or worker state is uncertain.");
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_dialog_state_transitions_do_not_silently_clear_data() {
        let mut s = LifecycleDialogState::default();
        s.request_clear(true);
        assert!(s.clear_confirmation_open);
        assert!(!s.clear_transition(ClearDialogAction::Cancel));
        s.request_clear(true);
        assert!(s.clear_transition(ClearDialogAction::ConfirmClear));
        assert!(!s.clear_confirmation_open);
    }
}
