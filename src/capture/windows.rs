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

#[cfg(windows)]
pub fn resolve_foreground_application(
) -> Result<super::foreground::ForegroundApplication, super::foreground::ForegroundError> {
    windows_foreground::cached_resolver().resolve_foreground()
}

#[cfg(not(windows))]
pub fn resolve_foreground_application(
) -> Result<super::foreground::ForegroundApplication, super::foreground::ForegroundError> {
    Ok(super::foreground::ForegroundApplication::unknown())
}

#[cfg(windows)]
mod windows_foreground {
    use super::super::foreground::{
        ApplicationIdentity, ForegroundApplication, ForegroundError, ForegroundResolver,
    };
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, HWND},
            System::{
                ProcessStatus::K32GetModuleFileNameExW,
                Threading::{
                    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                    PROCESS_VM_READ,
                },
            },
            UI::WindowsAndMessaging::{
                GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
            },
        },
    };

    #[derive(Clone)]
    struct ProcessMetadata {
        executable_name: String,
        executable_path: Option<String>,
        creation_low: u32,
        creation_high: u32,
    }

    #[derive(Default)]
    pub struct WindowsForegroundResolver {
        cache: HashMap<u32, ProcessMetadata>,
    }

    pub fn cached_resolver() -> std::sync::MutexGuard<'static, WindowsForegroundResolver> {
        static RESOLVER: OnceLock<Mutex<WindowsForegroundResolver>> = OnceLock::new();
        RESOLVER
            .get_or_init(|| Mutex::new(WindowsForegroundResolver::default()))
            .lock()
            .expect("foreground resolver lock")
    }

    impl ForegroundResolver for WindowsForegroundResolver {
        fn resolve_foreground(&mut self) -> Result<ForegroundApplication, ForegroundError> {
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd.0 == 0 {
                return Ok(ForegroundApplication::unknown());
            }
            let pid = window_pid(hwnd)
                .ok_or_else(|| ForegroundError("foreground PID unavailable".into()))?;
            let class = window_class(hwnd);
            let metadata = self
                .metadata_for_pid(pid)
                .unwrap_or_else(|| ProcessMetadata {
                    executable_name: "unknown/system".into(),
                    executable_path: None,
                    creation_low: 0,
                    creation_high: 0,
                });
            Ok(ForegroundApplication {
                identity: ApplicationIdentity::new(
                    pid,
                    metadata.executable_name.clone(),
                    metadata.executable_path.clone(),
                    class,
                ),
                display_label: Some(metadata.executable_name),
            })
        }
    }

    impl WindowsForegroundResolver {
        fn metadata_for_pid(&mut self, pid: u32) -> Option<ProcessMetadata> {
            let (path, low, high) = process_path_and_creation(pid)?;
            if let Some(cached) = self.cache.get(&pid) {
                if cached.creation_low == low && cached.creation_high == high {
                    return Some(cached.clone());
                }
            }
            let name = path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown/system")
                .to_owned();
            let metadata = ProcessMetadata {
                executable_name: name,
                executable_path: path,
                creation_low: low,
                creation_high: high,
            };
            self.cache.insert(pid, metadata.clone());
            Some(metadata)
        }
    }

    fn window_pid(hwnd: HWND) -> Option<u32> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        (pid != 0).then_some(pid)
    }

    fn window_class(hwnd: HWND) -> Option<String> {
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }

    fn process_path_and_creation(pid: u32) -> Option<(Option<String>, u32, u32)> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            )
            .ok()?
        };
        let mut creation = Default::default();
        let mut exit = Default::default();
        let mut kernel = Default::default();
        let mut user = Default::default();
        let _ =
            unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
        let mut buf = [0u16; 32768];
        let len = unsafe {
            K32GetModuleFileNameExW(handle, None, PWSTR(buf.as_mut_ptr()), buf.len() as u32)
        };
        unsafe {
            let _ = CloseHandle(handle);
        }
        let path = (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]));
        Some((path, creation.dwLowDateTime, creation.dwHighDateTime))
    }
}
