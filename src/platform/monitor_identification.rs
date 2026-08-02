//! Native, non-activating monitor identification overlays.
//!
//! Geometry, numbering and session state are deliberately platform neutral.  Only
//! `WindowsBackend` contains Win32 code, so the policy can be tested everywhere.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub const DISPLAY_TIME: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorDescriptor {
    pub stable_id: String,
    pub windows_name: String,
    pub number: u32,
    pub rect: PixelRect,
}

pub fn terminal_display_number(name: &str) -> Option<u32> {
    let visible = name.strip_prefix(r"\\.\").unwrap_or(name);
    let digits = visible.strip_prefix("DISPLAY")?;
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n = digits.parse().ok()?;
    (n != 0).then_some(n)
}

pub fn visible_display_name(name: &str, number: u32) -> String {
    terminal_display_number(name)
        .map(|n| format!("DISPLAY{n}"))
        .unwrap_or_else(|| format!("DISPLAY{number}"))
}

/// Assign numbers independently of display enumeration order. Duplicate suffixes
/// are awarded to the deterministic first display; all others use free numbers.
pub fn assign_friendly_numbers(
    mut monitors: Vec<(String, String, PixelRect)>,
) -> Vec<MonitorDescriptor> {
    monitors.sort_by(|a, b| (a.2.top, a.2.left, &a.0, &a.1).cmp(&(b.2.top, b.2.left, &b.0, &b.1)));
    let mut claims: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, (_, name, _)) in monitors.iter().enumerate() {
        if let Some(n) = terminal_display_number(name) {
            claims.entry(n).or_default().push(i);
        }
    }
    let mut assigned = vec![None; monitors.len()];
    let mut used = BTreeSet::new();
    for (number, indexes) in claims {
        assigned[indexes[0]] = Some(number);
        used.insert(number);
    }
    let mut next = 1;
    for number in &mut assigned {
        if number.is_none() {
            while used.contains(&next) {
                next = next.saturating_add(1);
            }
            *number = Some(next);
            used.insert(next);
        }
    }
    monitors
        .into_iter()
        .zip(assigned)
        .map(
            |((stable_id, windows_name, rect), number)| MonitorDescriptor {
                stable_id,
                windows_name,
                number: number.unwrap(),
                rect,
            },
        )
        .collect()
}

pub fn inset_rect(rect: &PixelRect, requested: i32) -> PixelRect {
    let width = rect.right.saturating_sub(rect.left).max(1);
    let height = rect.bottom.saturating_sub(rect.top).max(1);
    let inset = requested
        .max(0)
        .min(16)
        .min((width - 1) / 2)
        .min((height - 1) / 2);
    PixelRect {
        left: rect.left.saturating_add(inset),
        top: rect.top.saturating_add(inset),
        right: rect.right.saturating_sub(inset),
        bottom: rect.bottom.saturating_sub(inset),
    }
}

pub trait NativeWindowBackend: Send + 'static {
    type Handle: Copy + 'static;
    fn create(&mut self, monitor: &MonitorDescriptor) -> Result<Self::Handle, String>;
    fn destroy(&mut self, handle: Self::Handle) -> Result<(), String>;
    fn escape_pressed(&mut self) -> bool {
        false
    }
    fn pump_messages(&mut self) {}
}

#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
struct Session<B: NativeWindowBackend> {
    backend: B,
    handles: Vec<B::Handle>,
    generation: u64,
    deadline: Option<Instant>,
}
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
impl<B: NativeWindowBackend> Session<B> {
    fn new(backend: B) -> Self {
        Self {
            backend,
            handles: vec![],
            generation: 0,
            deadline: None,
        }
    }
    fn close(&mut self) -> Vec<String> {
        self.deadline = None;
        self.handles
            .drain(..)
            .filter_map(|h| self.backend.destroy(h).err())
            .collect()
    }
    fn show(&mut self, monitors: &[MonitorDescriptor], now: Instant) -> Result<u64, String> {
        let cleanup = self.close();
        self.generation = self.generation.wrapping_add(1);
        for monitor in monitors {
            match self.backend.create(monitor) {
                Ok(h) => self.handles.push(h),
                Err(error) => {
                    let mut errors = vec![error];
                    errors.extend(self.close());
                    return Err(errors.join("; "));
                }
            }
        }
        self.deadline = Some(now + DISPLAY_TIME);
        if cleanup.is_empty() {
            Ok(self.generation)
        } else {
            Err(format!(
                "overlays replaced after cleanup errors: {}",
                cleanup.join("; ")
            ))
        }
    }
    fn tick(&mut self, now: Instant) -> Vec<String> {
        self.backend.pump_messages();
        if self.deadline.is_some_and(|d| now >= d)
            || (!self.handles.is_empty() && self.backend.escape_pressed())
        {
            self.close()
        } else {
            vec![]
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Show(Vec<MonitorDescriptor>),
    Close,
    Shutdown,
}
#[derive(Clone, Debug)]
pub enum Status {
    Shown {
        generation: u64,
        expires_at: Instant,
    },
    Closed,
    Error(String),
    ShutdownComplete,
}

pub struct WorkerController {
    commands: mpsc::SyncSender<Command>,
    statuses: mpsc::Receiver<Status>,
    join: Option<thread::JoinHandle<()>>,
    pending: bool,
}
impl WorkerController {
    pub fn new() -> Result<Self, String> {
        platform_controller()
    }
    #[allow(dead_code)]
    fn spawn<B: NativeWindowBackend>(backend: B) -> Self {
        let (tx, rx) = mpsc::sync_channel(4);
        let (status_tx, statuses) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut session = Session::new(backend);
            let mut running = true;
            while running {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Command::Show(monitors)) => match session.show(&monitors, Instant::now()) {
                        Ok(generation) => {
                            let _ = status_tx.send(Status::Shown {
                                generation,
                                expires_at: session.deadline.unwrap(),
                            });
                        }
                        Err(e) => {
                            let _ = status_tx.send(Status::Error(e));
                        }
                    },
                    Ok(Command::Close) => {
                        for e in session.close() {
                            let _ = status_tx.send(Status::Error(e));
                        }
                        let _ = status_tx.send(Status::Closed);
                    }
                    Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        for e in session.close() {
                            let _ = status_tx.send(Status::Error(e));
                        }
                        running = false;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let was_open = session.deadline.is_some();
                for e in session.tick(Instant::now()) {
                    let _ = status_tx.send(Status::Error(e));
                }
                if was_open && session.deadline.is_none() {
                    let _ = status_tx.send(Status::Closed);
                }
            }
            let _ = status_tx.send(Status::ShutdownComplete);
        });
        Self {
            commands: tx,
            statuses,
            join: Some(join),
            pending: false,
        }
    }
    pub fn send(&mut self, command: Command) -> Result<(), String> {
        self.pending = true;
        self.commands
            .try_send(command)
            .map_err(|e| format!("identification worker busy: {e}"))
    }
    pub fn try_status(&mut self) -> Option<Status> {
        let s = self.statuses.try_recv().ok()?;
        self.pending = matches!(s, Status::Shown { .. });
        Some(s)
    }
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    pub fn shutdown(&mut self) {
        let _ = self.commands.try_send(Command::Shutdown);
    }
}
impl Drop for WorkerController {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_controller() -> Result<WorkerController, String> {
    Err("monitor identification is unsupported on this platform".into())
}

#[cfg(target_os = "windows")]
fn platform_controller() -> Result<WorkerController, String> {
    Ok(WorkerController::spawn(WindowsBackend))
}

#[cfg(target_os = "windows")]
struct WindowsBackend;
#[cfg(target_os = "windows")]
impl NativeWindowBackend for WindowsBackend {
    type Handle = isize;
    fn create(&mut self, monitor: &MonitorDescriptor) -> Result<Self::Handle, String> {
        windows_native::create(monitor).map(|h| h.0 as isize)
    }
    fn destroy(&mut self, handle: Self::Handle) -> Result<(), String> {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::DestroyWindow(
                windows::Win32::Foundation::HWND(handle as *mut _),
            )
            .map_err(|e| e.to_string())
        }
    }
    fn escape_pressed(&mut self) -> bool {
        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x1b) < 0 }
    }
    fn pump_messages(&mut self) {
        windows_native::pump()
    }
}

#[cfg(target_os = "windows")]
mod windows_native {
    use super::*;
    use std::sync::OnceLock;
    use windows::{
        core::w,
        Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    };
    static CLASS: OnceLock<Result<(), String>> = OnceLock::new();
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let number = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32;
            let palette = [0x003b2f68, 0x00602c3e, 0x002c5a50, 0x00604424];
            let brush = CreateSolidBrush(COLORREF(palette[number as usize % palette.len()]));
            FillRect(dc, &rc, brush);
            let _ = DeleteObject(brush);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00ffffff));
            let big = format!("{number}");
            let mut big: Vec<u16> = big.encode_utf16().collect();
            let midpoint = rc.bottom * 2 / 3;
            let mut number_rect = RECT {
                bottom: midpoint,
                ..rc
            };
            DrawTextW(
                dc,
                &mut big,
                &mut number_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            let mut title = [0u16; 64];
            let length = GetWindowTextW(hwnd, &mut title);
            let mut name_rect = RECT {
                top: midpoint,
                ..rc
            };
            DrawTextW(
                dc,
                &mut title[..length as usize],
                &mut name_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
    fn register() -> Result<(), String> {
        CLASS
            .get_or_init(|| unsafe {
                let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                    .map_err(|e| e.to_string())?;
                let class = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance.into(),
                    lpszClassName: w!("MultiMouseCanvasMonitorId"),
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    ..Default::default()
                };
                if RegisterClassW(&class) == 0 {
                    return Err(windows::core::Error::from_win32().to_string());
                }
                Ok(())
            })
            .clone()
    }
    pub fn create(m: &MonitorDescriptor) -> Result<HWND, String> {
        register()?;
        let r = inset_rect(&m.rect, 8);
        unsafe {
            let title: Vec<u16> = visible_display_name(&m.windows_name, m.number)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("MultiMouseCanvasMonitorId"),
                windows::core::PCWSTR(title.as_ptr()),
                WS_POPUP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, m.number as isize);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            Ok(hwnd)
        }
    }
    pub fn pump() {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suffix_parsing() {
        assert_eq!(terminal_display_number(r"\\.\DISPLAY1"), Some(1));
        assert_eq!(terminal_display_number("DISPLAY12"), Some(12));
        for n in ["", "DISPLAY", "THING1", "DISPLAY1x", "DISPLAY42949672960"] {
            assert_eq!(terminal_display_number(n), None);
        }
    }
    fn r(l: i32, t: i32, w: i32, h: i32) -> PixelRect {
        PixelRect {
            left: l,
            top: t,
            right: l + w,
            bottom: t + h,
        }
    }
    #[test]
    fn numbering_is_stable_and_resolves_duplicates() {
        let a = vec![
            ("b".into(), "DISPLAY2".into(), r(-10, 0, 10, 10)),
            ("a".into(), "DISPLAY2".into(), r(-10, 0, 10, 10)),
            ("c".into(), "virtual".into(), r(0, -20, 10, 10)),
        ];
        let mut b = a.clone();
        b.reverse();
        assert_eq!(
            assign_friendly_numbers(a),
            assign_friendly_numbers(b.clone())
        );
        let out = assign_friendly_numbers(b);
        assert_eq!(
            out.iter().map(|m| m.number).collect::<BTreeSet<_>>().len(),
            3
        );
        assert_eq!(out[0].stable_id, "c");
    }
    #[test]
    fn rectangles_preserve_layout_and_never_invert() {
        for rect in [
            r(-1920, 0, 1920, 1080),
            r(0, -1080, 1920, 1080),
            r(0, 0, 1080, 1920),
            r(0, 0, 5120, 1440),
            r(i32::MAX - 1, i32::MIN, 1, 1),
        ] {
            let x = inset_rect(&rect, 16);
            assert!(x.right > x.left && x.bottom > x.top);
        }
    }
    #[derive(Default)]
    struct Fake {
        next: u8,
        destroyed: Vec<u8>,
        fail_at: Option<u8>,
    }
    impl NativeWindowBackend for Fake {
        type Handle = u8;
        fn create(&mut self, _: &MonitorDescriptor) -> Result<u8, String> {
            self.next += 1;
            if self.fail_at == Some(self.next) {
                Err("create".into())
            } else {
                Ok(self.next)
            }
        }
        fn destroy(&mut self, h: u8) -> Result<(), String> {
            self.destroyed.push(h);
            Ok(())
        }
    }
    fn d(id: &str) -> MonitorDescriptor {
        MonitorDescriptor {
            stable_id: id.into(),
            windows_name: "DISPLAY1".into(),
            number: 1,
            rect: r(0, 0, 10, 10),
        }
    }
    #[test]
    fn expiration_repeated_show_close_and_shutdown_policy() {
        let start = Instant::now();
        let mut s = Session::new(Fake::default());
        assert_eq!(s.show(&[d("a")], start), Ok(1));
        assert!(s.tick(start + Duration::from_secs(4)).is_empty());
        assert_eq!(
            s.show(&[d("b"), d("c")], start + Duration::from_secs(4)),
            Ok(2)
        );
        assert_eq!(s.backend.destroyed, vec![1]);
        s.tick(start + Duration::from_secs(8));
        assert_eq!(s.handles.len(), 2);
        s.tick(start + Duration::from_secs(9));
        assert!(s.handles.is_empty());
        assert!(s.close().is_empty());
        assert!(s.close().is_empty());
    }
    #[test]
    fn partial_creation_is_cleaned_up() {
        let mut s = Session::new(Fake {
            fail_at: Some(2),
            ..Default::default()
        });
        assert!(s.show(&[d("a"), d("b")], Instant::now()).is_err());
        assert_eq!(s.backend.destroyed, vec![1]);
        assert!(s.handles.is_empty());
    }
}
