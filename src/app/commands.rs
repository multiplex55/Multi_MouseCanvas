#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    Show,
    StartRecording,
    PauseRecording,
    ResumeRecording,
    TogglePauseResume,
    FinishSession,
    ExportCurrentCanvas,
    MinimizeToTray,
    Exit,
    /// Internal tray source marker (not part of the CLI/wire protocol).
    ExitFromTray,
}

impl AppCommand {
    pub fn wire_sources() -> &'static [&'static str] {
        &["ui", "tray", "ipc", "cli"]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    UnknownArgument(String),
    HelpRequested,
}

pub fn cli_help_text() -> &'static str {
    "MultiMouseCanvas command-line commands:\n\
     --show      Show the application window\n\
     --start     Start recording global mouse position samples\n\
     --pause     Pause recording without collecting mouse samples\n\
     --resume    Resume recording mouse position samples\n\
     --finish    Finish the session and stop recording\n\
     --export    Export the current canvas\n\
     --exit      Exit the running application\n\
     --help      Print this help text\n\n\
     Privacy: commands control mouse-position recording only; they do not collect clicks, keyboard input, screenshots, window contents, browser URLs, or window titles by default."
}

pub fn parse_cli_args<I, S>(args: I) -> Result<Vec<AppCommand>, CliParseError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|arg| match arg.as_ref() {
            "--start" => Ok(AppCommand::StartRecording),
            "--show" => Ok(AppCommand::Show),
            "--pause" => Ok(AppCommand::PauseRecording),
            "--resume" => Ok(AppCommand::ResumeRecording),
            "--finish" => Ok(AppCommand::FinishSession),
            "--export" => Ok(AppCommand::ExportCurrentCanvas),
            "--exit" => Ok(AppCommand::Exit),
            "--help" | "-h" => Err(CliParseError::HelpRequested),
            other => Err(CliParseError::UnknownArgument(other.to_owned())),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSessionOutcome {
    ClearPreviousCanvas,
    PreserveForExport,
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cli_arguments_into_app_commands() {
        assert_eq!(
            parse_cli_args([
                "--start", "--show", "--pause", "--resume", "--finish", "--export", "--exit"
            ])
            .unwrap(),
            vec![
                AppCommand::StartRecording,
                AppCommand::Show,
                AppCommand::PauseRecording,
                AppCommand::ResumeRecording,
                AppCommand::FinishSession,
                AppCommand::ExportCurrentCanvas,
                AppCommand::Exit,
            ]
        );
        assert_eq!(
            parse_cli_args(["--bad"]),
            Err(CliParseError::UnknownArgument("--bad".into()))
        );
    }
    #[test]
    fn ui_tray_and_cli_share_command_enum() {
        assert_eq!(AppCommand::wire_sources(), &["ui", "tray", "ipc", "cli"]);
    }

    #[test]
    fn cli_help_documents_privacy_sensitive_recording_commands() {
        let help = cli_help_text();
        assert!(help.contains("Start recording global mouse position samples"));
        assert!(help.contains("Pause recording without collecting mouse samples"));
        assert!(help.contains("Finish the session and stop recording"));
        assert!(help.contains("do not collect clicks"));
        assert!(help.contains("keyboard input"));
        assert!(help.contains("screenshots"));
        assert!(matches!(
            parse_cli_args(["--help"]),
            Err(CliParseError::HelpRequested)
        ));
    }
}
