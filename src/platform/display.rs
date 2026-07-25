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
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
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
            let primary = (info.monitorInfo.dwFlags & 1) != 0;
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
        enrich_display_identities(&mut monitors);
        Ok(DisplayTopology::new(monitors))
    }
}

#[cfg(windows)]
fn enrich_display_identities(monitors: &mut [Monitor]) {
    use crate::platform::display_identity::{AdapterLuid, DisplayOrientation};
    use windows::Win32::{Devices::Display::*, Foundation::ERROR_SUCCESS};
    unsafe {
        let (mut path_count, mut mode_count) = (0, 0);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
            != ERROR_SUCCESS
        {
            return;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        ) != ERROR_SUCCESS
        {
            return;
        }
        for path in paths.into_iter().take(path_count as usize) {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of_val(&source) as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            };
            if DisplayConfigGetDeviceInfo(&mut source.header) != 0 {
                continue;
            }
            let gdi = wide(&source.viewGdiDeviceName);
            let Some(monitor) = monitors
                .iter_mut()
                .find(|m| m.label.as_deref() == Some(gdi.as_str()))
            else {
                continue;
            };
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of_val(&target) as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            };
            monitor.identity.adapter_luid = Some(AdapterLuid {
                high: path.targetInfo.adapterId.HighPart,
                low: path.targetInfo.adapterId.LowPart,
            });
            monitor.identity.source_id = Some(path.sourceInfo.id);
            monitor.identity.target_id = Some(path.targetInfo.id);
            monitor.identity.gdi_source_name = gdi;
            monitor.orientation = match path.targetInfo.rotation {
                DISPLAYCONFIG_ROTATION_ROTATE90 => DisplayOrientation::Portrait,
                DISPLAYCONFIG_ROTATION_ROTATE180 => DisplayOrientation::LandscapeFlipped,
                DISPLAYCONFIG_ROTATION_ROTATE270 => DisplayOrientation::PortraitFlipped,
                _ => DisplayOrientation::Landscape,
            };
            if DisplayConfigGetDeviceInfo(&mut target.header) == 0 {
                monitor.identity.monitor_device_path =
                    Some(wide(&target.monitorDevicePath)).filter(|s| !s.is_empty());
                monitor.identity.target_friendly_name =
                    Some(wide(&target.monitorFriendlyDeviceName)).filter(|s| !s.is_empty());
                if target.edidManufactureId != 0 {
                    monitor.identity.edid_manufacturer_id = Some(target.edidManufactureId)
                }
                if target.edidProductCodeId != 0 {
                    monitor.identity.edid_product_code = Some(target.edidProductCodeId)
                }
            }
            // Even a failed target query retains adapter/target identity and the GDI monitor.
            monitor.identity.rebuild_stable_key();
            monitor.id = monitor.identity.stable_key.clone();
        }
    }
}
#[cfg(windows)]
fn wide(v: &[u16]) -> String {
    String::from_utf16_lossy(&v[..v.iter().position(|c| *c == 0).unwrap_or(v.len())])
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
