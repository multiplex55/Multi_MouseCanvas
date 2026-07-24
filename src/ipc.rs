use crate::app::commands::AppCommand;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::Sender,
    thread,
};

pub const IPC_ADDR: &str = "127.0.0.1:47839";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceDecision {
    Primary,
    Forwarded,
    RejectedDuplicate,
}

pub trait CommandTransport {
    fn claim_primary(&mut self) -> bool;
    fn forward(&mut self, command: AppCommand) -> bool;
}

pub fn decide_instance<T: CommandTransport>(
    transport: &mut T,
    commands: &[AppCommand],
) -> InstanceDecision {
    if transport.claim_primary() {
        return InstanceDecision::Primary;
    }
    if !commands.is_empty() && commands.iter().copied().all(|c| transport.forward(c)) {
        InstanceDecision::Forwarded
    } else {
        InstanceDecision::RejectedDuplicate
    }
}

pub struct TcpCommandTransport;
impl CommandTransport for TcpCommandTransport {
    fn claim_primary(&mut self) -> bool {
        TcpListener::bind(IPC_ADDR).is_ok()
    }
    fn forward(&mut self, command: AppCommand) -> bool {
        forward_command(command).is_ok()
    }
}

pub fn bind_listener() -> std::io::Result<TcpListener> {
    TcpListener::bind(IPC_ADDR)
}

pub fn serve(listener: TcpListener, tx: Sender<AppCommand>) {
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Some(c) = read_command(stream) {
                let _ = tx.send(c);
            }
        }
    });
}

fn read_command(mut stream: TcpStream) -> Option<AppCommand> {
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    parse_wire_command(buf.trim())
}

pub fn forward_command(command: AppCommand) -> std::io::Result<()> {
    let mut s = TcpStream::connect(IPC_ADDR)?;
    s.write_all(format!("{}\n", format_wire_command(command)).as_bytes())
}

fn format_wire_command(command: AppCommand) -> &'static str {
    match command {
        AppCommand::Show => "show",
        AppCommand::StartRecording => "start",
        AppCommand::PauseRecording => "pause",
        AppCommand::ResumeRecording => "resume",
        AppCommand::TogglePauseResume => "toggle_pause_resume",
        AppCommand::FinishSession => "finish",
        AppCommand::ExportCurrentCanvas => "export",
        AppCommand::Exit => "exit",
    }
}
fn parse_wire_command(s: &str) -> Option<AppCommand> {
    Some(match s {
        "show" => AppCommand::Show,
        "start" => AppCommand::StartRecording,
        "pause" => AppCommand::PauseRecording,
        "resume" => AppCommand::ResumeRecording,
        "toggle_pause_resume" => AppCommand::TogglePauseResume,
        "finish" => AppCommand::FinishSession,
        "export" => AppCommand::ExportCurrentCanvas,
        "exit" => AppCommand::Exit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake {
        claimed: bool,
        forwards: Vec<AppCommand>,
    }
    impl CommandTransport for Fake {
        fn claim_primary(&mut self) -> bool {
            self.claimed
        }
        fn forward(&mut self, c: AppCommand) -> bool {
            self.forwards.push(c);
            true
        }
    }
    #[test]
    fn forwarding_logic_uses_fake_transport() {
        let mut f = Fake {
            claimed: false,
            forwards: vec![],
        };
        assert_eq!(
            decide_instance(&mut f, &[AppCommand::StartRecording]),
            InstanceDecision::Forwarded
        );
        assert_eq!(f.forwards, vec![AppCommand::StartRecording]);
    }
    #[test]
    fn duplicate_without_command_is_rejected() {
        let mut f = Fake {
            claimed: false,
            forwards: vec![],
        };
        assert_eq!(
            decide_instance(&mut f, &[]),
            InstanceDecision::RejectedDuplicate
        );
    }
    #[test]
    fn duplicate_recorder_startup_is_forwarded() {
        let mut f = Fake {
            claimed: false,
            forwards: vec![],
        };
        assert_eq!(
            decide_instance(&mut f, &[AppCommand::StartRecording]),
            InstanceDecision::Forwarded
        );
    }
}
