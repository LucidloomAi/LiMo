//! WebView2's own settings that Tauri does not pass on: the browser's
//! accelerator keys (F5, Ctrl+P, Ctrl+F and the rest) and its right click
//! menu, both switched off; and reading them back, for the self check.

use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Controller, ICoreWebView2Settings, ICoreWebView2Settings3};
use windows::Win32::Foundation::HWND;
use windows::core::{BOOL, Interface};

/// What WebView2 says its settings are. None where it would not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SettingsSeen {
    /// The browser's keys: reload, print, find, zoom, developer tools.
    pub accelerator_keys: Option<bool>,
    /// The right click menu.
    pub context_menus: Option<bool>,
    /// The developer tools.
    pub devtools: Option<bool>,
    /// Zooming with Ctrl and the wheel or keys.
    pub zoom: Option<bool>,
    /// The status bar that shows a link's address.
    pub status_bar: Option<bool>,
}

fn settings(controller: &ICoreWebView2Controller) -> windows::core::Result<ICoreWebView2Settings> {
    // SAFETY: COM calls on interfaces Tauri handed over, on the window's own
    // thread (Tauri runs `with_webview` there).
    unsafe { controller.CoreWebView2()?.Settings() }
}

/// Switches off the browser's accelerator keys and its right click menu.
/// With `keep_keys`, leaves both as WebView2 has them (the self check's
/// control, which shows what the probe can see).
pub fn lock_down(controller: &ICoreWebView2Controller, keep_keys: bool) -> Result<(), String> {
    let settings = settings(controller).map_err(|e| e.message())?;
    if keep_keys {
        return Ok(());
    }
    // SAFETY: as in `settings`; each setter takes a plain value.
    unsafe {
        settings.SetAreDefaultContextMenusEnabled(false).map_err(|e| e.message())?;
        let third: ICoreWebView2Settings3 = settings.cast().map_err(|e| e.message())?;
        third.SetAreBrowserAcceleratorKeysEnabled(false).map_err(|e| e.message())?;
    }
    Ok(())
}

/// Reads the settings back.
pub fn read_settings(controller: &ICoreWebView2Controller) -> SettingsSeen {
    let Ok(settings) = settings(controller) else { return SettingsSeen::default() };
    let read = |f: &dyn Fn(*mut BOOL) -> windows::core::Result<()>| {
        let mut value = BOOL(0);
        f(&mut value).ok().map(|()| value.as_bool())
    };
    // SAFETY: each getter writes one BOOL into the value handed to it.
    unsafe {
        let third: Option<ICoreWebView2Settings3> = settings.cast().ok();
        SettingsSeen {
            accelerator_keys: third.and_then(|t| read(&|p| t.AreBrowserAcceleratorKeysEnabled(p))),
            context_menus: read(&|p| settings.AreDefaultContextMenusEnabled(p)),
            devtools: read(&|p| settings.AreDevToolsEnabled(p)),
            zoom: read(&|p| settings.IsZoomControlEnabled(p)),
            status_bar: read(&|p| settings.IsStatusBarEnabled(p)),
        }
    }
}

/// The window WebView2 draws into, as a number, for the key probe.
pub fn parent_window(controller: &ICoreWebView2Controller) -> Option<isize> {
    let mut window = HWND::default();
    // SAFETY: writes the window's handle into the value handed to it.
    unsafe { controller.ParentWindow(&mut window).ok()? };
    (!window.is_invalid()).then_some(window.0 as isize)
}
