use crate::session::model::RecordingStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub enabled: bool,
    pub label: &'static str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPolicy {
    pub start: Action,
    pub pause_resume: Action,
    pub finish: Action,
    pub export: Action,
    pub clear: Action,
    pub edit_profiles: Action,
    pub edit_settings: Action,
}
pub fn action_policy(
    status: RecordingStatus,
    pending: bool,
    export_busy: bool,
    retained_canvas: bool,
    connected: bool,
    shutting_down: bool,
) -> ActionPolicy {
    let available = connected && !shutting_down && !pending;
    ActionPolicy {
        start: Action {
            enabled: available && status == RecordingStatus::Stopped,
            label: if pending { "Starting…" } else { "Start" },
        },
        pause_resume: Action {
            enabled: available
                && matches!(status, RecordingStatus::Recording | RecordingStatus::Paused),
            label: if pending {
                if status == RecordingStatus::Paused {
                    "Resuming…"
                } else {
                    "Pausing…"
                }
            } else if status == RecordingStatus::Paused {
                "Resume"
            } else {
                "Pause"
            },
        },
        finish: Action {
            enabled: available
                && matches!(status, RecordingStatus::Recording | RecordingStatus::Paused),
            label: if pending { "Finishing…" } else { "Finish" },
        },
        export: Action {
            enabled: available && retained_canvas && !export_busy,
            label: "Export PNG",
        },
        clear: Action {
            enabled: available
                && matches!(status, RecordingStatus::Stopped | RecordingStatus::Finished)
                && retained_canvas,
            label: if pending { "Clearing…" } else { "Clear" },
        },
        edit_profiles: Action {
            enabled: available
                && matches!(status, RecordingStatus::Stopped | RecordingStatus::Finished),
            label: "Profiles",
        },
        edit_settings: Action {
            enabled: available,
            label: "Settings",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_table_is_consistent() {
        let stopped = action_policy(RecordingStatus::Stopped, false, false, false, true, false);
        assert!(stopped.start.enabled);
        assert!(!stopped.pause_resume.enabled && !stopped.finish.enabled);
        let recording = action_policy(RecordingStatus::Recording, false, false, true, true, false);
        assert!(
            !recording.start.enabled && recording.pause_resume.enabled && recording.finish.enabled
        );
        let paused = action_policy(RecordingStatus::Paused, false, false, true, true, false);
        assert_eq!(paused.pause_resume.label, "Resume");
        let finished = action_policy(RecordingStatus::Finished, false, false, true, true, false);
        assert!(finished.export.enabled && finished.clear.enabled && !finished.start.enabled);
    }
    #[test]
    fn pending_disables_every_mutating_or_export_action() {
        let p = action_policy(RecordingStatus::Recording, true, false, true, true, false);
        assert!(
            !p.start.enabled
                && !p.pause_resume.enabled
                && !p.finish.enabled
                && !p.export.enabled
                && !p.clear.enabled
        );
    }
}
