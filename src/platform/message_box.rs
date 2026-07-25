use std::sync::atomic::{AtomicBool, Ordering};
static SHOWN: AtomicBool = AtomicBool::new(false);

pub fn show_fatal(message: &str) {
    if SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    show("MultiMouseCanvas", message);
}

#[cfg(windows)]
fn show(title: &str, message: &str) {
    use windows::{
        core::PCWSTR,
        Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
    };
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
#[cfg(not(windows))]
fn show(_: &str, _: &str) {}

#[cfg(windows)]
pub fn debug_output(message: &str) {
    use windows::{core::PCWSTR, Win32::System::Diagnostics::Debug::OutputDebugStringW};
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OutputDebugStringW(PCWSTR(message.as_ptr()));
    }
}
#[cfg(not(windows))]
pub fn debug_output(_: &str) {}
