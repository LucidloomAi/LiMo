//! Enumerate the attached monitors.

use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

/// One monitor: its pixel rectangle and whether it is the primary display.
#[derive(Debug, Clone, Copy)]
pub struct MonitorRec {
    pub rect: RECT,
    pub primary: bool,
}

/// List every monitor currently attached, in the order the system reports them.
pub fn list_monitors() -> Vec<MonitorRec> {
    let mut found: Vec<MonitorRec> = Vec::new();
    // SAFETY: we pass a valid callback and a pointer to our own Vec as the
    // user data. EnumDisplayMonitors calls the callback synchronously on this
    // thread, so the borrow is sound for the duration of the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_proc),
            LPARAM(&mut found as *mut Vec<MonitorRec> as isize),
        );
    }
    found
}

unsafe extern "system" fn enum_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let found = &mut *(data.0 as *mut Vec<MonitorRec>);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(hmon, &mut info).as_bool() {
        found.push(MonitorRec {
            rect: info.rcMonitor,
            primary: (info.dwFlags & MONITORINFOF_PRIMARY) != 0,
        });
    }
    BOOL(1) // keep enumerating
}
