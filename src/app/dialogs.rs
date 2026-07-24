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
pub fn show(ctx: &egui::Context, state: &mut AppState, confirm_exit: &mut bool) {
    if state.pending_new_session_decision {
        egui::Window::new("Start new session?").collapsible(false).show(ctx, |ui| { ui.label("Existing activity is present. Choose how to proceed; no unexported canvas data will be silently deleted."); ui.horizontal(|ui| { if ui.button("Resume if paused").clicked(){state.resolve_new_session(NewSessionOutcome::Cancel);} if ui.button("Preserve recovery and start new session").clicked(){state.resolve_new_session(NewSessionOutcome::ClearPreviousCanvas);} if ui.button("Export and start new session").clicked(){state.apply_command(crate::app::commands::AppCommand::ExportCurrentCanvas); state.resolve_new_session(NewSessionOutcome::ClearPreviousCanvas);} if ui.button("Cancel").clicked(){state.resolve_new_session(NewSessionOutcome::Cancel);} }); });
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
    if *confirm_exit {
        egui::Window::new("Exit MultiMouseCanvas?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Exit and stop any background recording/sampling?");
                ui.horizontal(|ui| {
                    if ui.button("Exit").clicked() {
                        state.exit_requested = true;
                    }
                    if ui.button("Cancel").clicked() {
                        *confirm_exit = false;
                    }
                });
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
