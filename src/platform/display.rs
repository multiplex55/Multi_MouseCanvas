use crate::canvas::topology::DisplayTopology;
#[cfg(windows)]
use crate::canvas::{coordinates::DesktopRect, topology::Monitor};

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("display enumeration unavailable on this platform")]
    Unavailable,
}

pub fn set_process_dpi_awareness() {
    platform_set_process_dpi_awareness();
}
pub fn current_topology() -> Result<DisplayTopology, DisplayError> {
    platform_current_topology()
}

#[cfg(windows)]
fn platform_set_process_dpi_awareness() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
#[cfg(not(windows))]
fn platform_set_process_dpi_awareness() {}

#[cfg(windows)]
fn platform_current_topology() -> Result<DisplayTopology, DisplayError> {
    use windows::Win32::{
        Foundation::{BOOL, LPARAM, RECT},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
            MONITORINFOF_PRIMARY,
        },
    };
    unsafe extern "system" fn enum_proc(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = &mut *(lparam.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _).as_bool() {
            let r = info.monitorInfo.rcMonitor;
            let rect =
                DesktopRect::new(r.left as f32, r.top as f32, r.right as f32, r.bottom as f32);
            let name = String::from_utf16_lossy(&info.szDevice)
                .trim_matches(char::from(0))
                .to_owned();
            let primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY.0) != 0;
            let mut m = Monitor::new(
                if name.is_empty() {
                    format!("HMONITOR{:?}", monitor.0)
                } else {
                    name.clone()
                },
                rect,
                primary,
            );
            m.label = (!name.is_empty()).then_some(name);
            monitors.push(m);
        }
        BOOL(1)
    }
    let mut monitors = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(enum_proc),
            LPARAM(&mut monitors as *mut _ as isize),
        )
        .ok()
        .map_err(|_| DisplayError::Unavailable)?;
    }
    if monitors.is_empty() {
        Err(DisplayError::Unavailable)
    } else {
        Ok(DisplayTopology::new(monitors))
    }
}
#[cfg(not(windows))]
fn platform_current_topology() -> Result<DisplayTopology, DisplayError> {
    Ok(DisplayTopology::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_topology_has_bounds() {
        assert!(current_topology().unwrap().bounds().is_some());
    }
}
