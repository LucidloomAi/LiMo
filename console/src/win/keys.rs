//! The self check's key probe: F5, Ctrl+P, Ctrl+F and a right click, sent
//! to WebView2's own window inside the console's window, and nothing else.
//!
//! Each is a message posted to that one window, never input into the PC
//! (no `SendInput`), so nothing reaches another program, the guardian's
//! own typing is never mixed in, and the console never has to come to the
//! front.
//!
//! What a posted message can show is limited, and README.md says how: it
//! reaches the page as F5 and as a right click, but a letter or Ctrl does
//! not get through, and WebView2 never acts on a posted F5 even with its
//! keys left on (the control run). So the probe shows what reached the page
//! and that nothing opened; that the keys do nothing rests on WebView2's own
//! settings, read back, and on the page's guard.
//!
//! What it finds is only what the windows say: the probe reports which
//! windows of WebView2's processes were showing before and after, and the
//! page reports what reached it.

use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_CONTROL, VK_F5};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetClientRect, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_KEYDOWN, WM_KEYUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
};
use windows::core::BOOL;

use super::from_wide;

/// `MK_RBUTTON`: the right button is down, in a mouse message's wParam.
const MK_RBUTTON: usize = 0x0002;

/// A window: its handle as a number, its class, its process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSeen {
    /// The handle, as a number.
    pub handle: isize,
    /// Its class name, such as `Chrome_RenderWidgetHostHWND`.
    pub class: String,
    /// Its process.
    pub pid: u32,
}

fn class_of(window: HWND) -> String {
    let mut name = [0u16; 256];
    // SAFETY: writes at most the buffer's length.
    let n = unsafe { GetClassNameW(window, &mut name) };
    from_wide(&name[..n.max(0) as usize])
}

fn pid_of(window: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: writes the process id into the value handed to it.
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    pid
}

unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
    // SAFETY: `list` is the Vec handed to the enumeration below, alive for
    // its length.
    let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    list.push(window);
    BOOL(1)
}

/// Every window inside `parent`, at any depth, WebView2's among them.
pub fn children(parent: isize) -> Vec<WindowSeen> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: the callback writes only into `found`, which outlives the call.
    unsafe {
        let _ = EnumChildWindows(Some(HWND(parent as *mut _)), Some(collect), LPARAM(&mut found as *mut Vec<HWND> as isize));
    }
    found.into_iter().map(|w| WindowSeen { handle: w.0 as isize, class: class_of(w), pid: pid_of(w) }).collect()
}

/// Every visible top level window of the processes `pids`: where a menu, a
/// find bar or a print dialog of WebView2's would show.
pub fn visible_windows_of(pids: &[u32]) -> Vec<WindowSeen> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: as in `children`.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut found as *mut Vec<HWND> as isize));
    }
    found
        .into_iter()
        // SAFETY: only asks whether a window shows.
        .filter(|w| unsafe { IsWindowVisible(*w) }.as_bool() && pids.contains(&pid_of(*w)))
        .map(|w| WindowSeen { handle: w.0 as isize, class: class_of(w), pid: pid_of(w) })
        .collect()
}

/// The window a key should go to: WebView2's render widget window, or else
/// its widget window.
pub fn input_window(parent: isize) -> Option<WindowSeen> {
    let all = children(parent);
    ["Chrome_RenderWidgetHostHWND", "Chrome_WidgetWin_1", "Chrome_WidgetWin_0"]
        .iter()
        .find_map(|class| all.iter().find(|w| w.class == *class).cloned())
}

fn key_lparam(vk: u16, up: bool) -> LPARAM {
    // SAFETY: a pure lookup of the key's scan code.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } & 0xFF;
    let mut bits: u32 = 1 | (scan << 16);
    if up {
        bits |= (1 << 30) | (1 << 31);
    }
    LPARAM(bits as i32 as isize)
}

fn post(window: HWND, message: u32, wparam: usize, lparam: LPARAM) {
    // SAFETY: posts a message to one window; nothing waits on it.
    unsafe {
        let _ = PostMessageW(Some(window), message, WPARAM(wparam), lparam);
    }
}

/// A key pressed and let go, posted to `window`.
fn tap(window: HWND, vk: u16) {
    post(window, WM_KEYDOWN, usize::from(vk), key_lparam(vk, false));
    std::thread::sleep(Duration::from_millis(30));
    post(window, WM_KEYUP, usize::from(vk), key_lparam(vk, true));
}

/// Ctrl and a key, posted to `window`: Ctrl down, the key, Ctrl up.
fn tap_with_ctrl(window: HWND, vk: u16) {
    post(window, WM_KEYDOWN, usize::from(VK_CONTROL.0), key_lparam(VK_CONTROL.0, false));
    tap(window, vk);
    post(window, WM_KEYUP, usize::from(VK_CONTROL.0), key_lparam(VK_CONTROL.0, true));
}

/// A right click in the middle of `window`, posted to it.
fn right_click(window: HWND) {
    let mut rect = RECT::default();
    // SAFETY: writes the window's client size into the value handed to it.
    let _ = unsafe { GetClientRect(window, &mut rect) };
    let (x, y) = ((rect.right / 2).max(1), (rect.bottom / 2).max(1));
    let at = LPARAM(((y as u32 & 0xFFFF) << 16 | (x as u32 & 0xFFFF)) as isize);
    post(window, WM_RBUTTONDOWN, MK_RBUTTON, at);
    std::thread::sleep(Duration::from_millis(30));
    post(window, WM_RBUTTONUP, 0, at);
}

/// One thing the probe sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// F5: reload.
    F5,
    /// Ctrl+P: print.
    CtrlP,
    /// Ctrl+F: find on the page.
    CtrlF,
    /// A right click: the context menu.
    RightClick,
    /// A plain letter, X: whether a posted letter reaches the page at all.
    PlainX,
}

/// Sends one of them to `window`.
pub fn send(window: isize, probe: Probe) {
    let window = HWND(window as *mut _);
    match probe {
        Probe::F5 => tap(window, VK_F5.0),
        Probe::CtrlP => tap_with_ctrl(window, u16::from(b'P')),
        Probe::CtrlF => tap_with_ctrl(window, u16::from(b'F')),
        Probe::RightClick => right_click(window),
        Probe::PlainX => tap(window, u16::from(b'X')),
    }
}
