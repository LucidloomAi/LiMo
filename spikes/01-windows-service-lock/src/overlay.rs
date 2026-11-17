//! The on screen lock. One borderless topmost window covers each monitor, a
//! low level keyboard hook swallows keystrokes while the lock is up, and the
//! windows re assert themselves a few times a second.
//!
//! Two modes share this code:
//!   developer: runs for a fixed number of seconds, the chord Ctrl+Shift+F12
//!              ends it early with exit code 3.
//!   managed:   reads the deadline from the control file on every 250 ms
//!              timer tick and ends when the deadline passes, is cleared, or
//!              is further ahead than the 120 second cap. The escape chord is
//!              disabled. A hard cap ends it after 120 seconds no matter what
//!              the control file says.

use std::cell::RefCell;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HFONT, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_F12, VK_LCONTROL, VK_LSHIFT, VK_RCONTROL, VK_RSHIFT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, LoadCursorW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetTimer,
    SetWindowPos, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HWND_TOPMOST,
    IDC_ARROW, KBDLLHOOKSTRUCT, LLKHF_UP, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, WH_KEYBOARD_LL, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_TIMER,
    WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

use crate::config::{
    control_path, now_unix, parse_deadline, remaining_secs, sane_deadline, HELPER_CAP_SECS,
};
use crate::monitors::list_monitors;

const TITLE: &str = "LiMo spike: locked";

const BG_COLOR: COLORREF = COLORREF(0x0014_1414);
const FG_COLOR: COLORREF = COLORREF(0x00ED_EDED);

const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 250;

// Shared with the keyboard hook, which runs on this same thread.
static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
static SHIFT_DOWN: AtomicBool = AtomicBool::new(false);
static ESCAPE_REQUESTED: AtomicBool = AtomicBool::new(false);
static ALLOW_ESCAPE: AtomicBool = AtomicBool::new(false);

// Seconds left, written by the timer and read by the painter.
static REMAINING_SECS: AtomicI64 = AtomicI64::new(0);

struct OverlayState {
    managed: bool,
    start: Instant,
    dev_seconds: u64,
    windows: Vec<HWND>,
    primary: HWND,
}

thread_local! {
    static STATE: RefCell<Option<OverlayState>> = const { RefCell::new(None) };
}

/// Run the overlay. Returns the process exit code (0 normal, 3 developer
/// escape chord).
pub fn run(managed: bool, dev_seconds: u64) -> i32 {
    unsafe { run_inner(managed, dev_seconds) }
}

unsafe fn run_inner(managed: bool, dev_seconds: u64) -> i32 {
    // Match the coordinate space the monitor rectangles are in, so the windows
    // land exactly on each physical display.
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    ALLOW_ESCAPE.store(!managed, Ordering::SeqCst);
    ESCAPE_REQUESTED.store(false, Ordering::SeqCst);

    let hinst: HINSTANCE = match GetModuleHandleW(None) {
        Ok(h) => h.into(),
        Err(_) => return 0,
    };

    let class_name = w!("LiMoSpikeOverlay");
    let brush = CreateSolidBrush(BG_COLOR);
    let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinst,
        hCursor: cursor,
        hbrBackground: brush,
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let monitors = list_monitors();
    let mut windows: Vec<HWND> = Vec::new();
    let mut primary: Option<HWND> = None;

    for m in &monitors {
        let width = m.rect.right - m.rect.left;
        let height = m.rect.bottom - m.rect.top;
        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("LiMo spike"),
            WS_POPUP | WS_VISIBLE,
            m.rect.left,
            m.rect.top,
            width,
            height,
            None,
            None,
            Some(hinst),
            None,
        ) {
            Ok(h) => h,
            Err(_) => continue,
        };
        if m.primary && primary.is_none() {
            primary = Some(hwnd);
        }
        windows.push(hwnd);
    }

    if windows.is_empty() {
        // No displays to cover. Nothing to prove, exit cleanly.
        return 0;
    }
    let primary = primary.unwrap_or(windows[0]);

    REMAINING_SECS.store(initial_remaining(managed, dev_seconds), Ordering::SeqCst);

    STATE.with(|s| {
        *s.borrow_mut() = Some(OverlayState {
            managed,
            start: Instant::now(),
            dev_seconds,
            windows: windows.clone(),
            primary,
        });
    });

    // A global low level keyboard hook, serviced by the message loop below.
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(hinst), 0).ok();

    SetTimer(Some(primary), TIMER_ID, TIMER_MS, None);

    for &hwnd in &windows {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    let _ = SetForegroundWindow(primary);

    let mut msg = MSG::default();
    loop {
        let got = GetMessageW(&mut msg, None, 0, 0);
        if got.0 <= 0 {
            break; // 0 is WM_QUIT, -1 is an error; either way we stop.
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let exit_code = msg.wParam.0 as i32;

    if let Some(h) = hook {
        let _ = UnhookWindowsHookEx(h);
    }
    for &hwnd in &windows {
        let _ = DestroyWindow(hwnd);
    }
    STATE.with(|s| *s.borrow_mut() = None);

    exit_code
}

fn initial_remaining(managed: bool, dev_seconds: u64) -> i64 {
    if managed {
        managed_remaining().min(HELPER_CAP_SECS as i64)
    } else {
        dev_seconds as i64
    }
}

/// Seconds left on the lock as the managed overlay sees it. A deadline further
/// ahead than the cap is ignored (treated as no lock), the same rule the
/// service and `status` use, so all three always agree.
fn managed_remaining() -> i64 {
    let raw = std::fs::read_to_string(control_path())
        .map(|s| parse_deadline(&s))
        .unwrap_or(0);
    let now = now_unix();
    remaining_secs(sane_deadline(raw, now), now)
}

unsafe extern "system" fn keyboard_hook(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let is_up = (kb.flags.0 & LLKHF_UP.0) != 0;
        let vk = kb.vkCode;

        if vk == VK_LCONTROL.0 as u32 || vk == VK_RCONTROL.0 as u32 || vk == VK_CONTROL.0 as u32 {
            CTRL_DOWN.store(!is_up, Ordering::SeqCst);
        } else if vk == VK_LSHIFT.0 as u32 || vk == VK_RSHIFT.0 as u32 || vk == VK_SHIFT.0 as u32 {
            SHIFT_DOWN.store(!is_up, Ordering::SeqCst);
        }

        if !is_up
            && vk == VK_F12.0 as u32
            && ALLOW_ESCAPE.load(Ordering::SeqCst)
            && CTRL_DOWN.load(Ordering::SeqCst)
            && SHIFT_DOWN.load(Ordering::SeqCst)
        {
            ESCAPE_REQUESTED.store(true, Ordering::SeqCst);
        }

        // Swallow the keystroke so it never reaches another window while the
        // lock is up. We have already updated our own modifier tracking above.
        return LRESULT(1);
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1), // we paint the whole client area ourselves
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            on_timer();
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),   // ignore Alt+F4 and the like
        WM_DESTROY => LRESULT(0), // teardown is driven from run_inner
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_timer() {
    // Take a snapshot and drop the borrow before any Win32 call, because
    // SetWindowPos can re enter this window procedure.
    let snap = STATE.with(|s| {
        s.borrow().as_ref().map(|st| {
            (
                st.managed,
                st.start,
                st.dev_seconds,
                st.windows.clone(),
                st.primary,
            )
        })
    });
    let Some((managed, start, dev_seconds, windows, primary)) = snap else {
        return;
    };

    let elapsed = start.elapsed().as_secs();

    if managed {
        let remaining = managed_remaining();
        REMAINING_SECS.store(remaining, Ordering::SeqCst);
        if remaining <= 0 || elapsed >= HELPER_CAP_SECS {
            PostQuitMessage(0);
            return;
        }
    } else {
        if ESCAPE_REQUESTED.load(Ordering::SeqCst) {
            PostQuitMessage(3);
            return;
        }
        let remaining = (dev_seconds as i64 - elapsed as i64).max(0);
        REMAINING_SECS.store(remaining, Ordering::SeqCst);
        if elapsed >= dev_seconds {
            PostQuitMessage(0);
            return;
        }
    }

    for &hwnd in &windows {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    let _ = SetForegroundWindow(primary);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    let bg = CreateSolidBrush(BG_COLOR);
    FillRect(hdc, &rc, bg);
    let _ = DeleteObject(bg.into());

    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, FG_COLOR);

    let height = (rc.bottom - rc.top).max(1);
    let title_size = (height / 10).clamp(28, 96);
    let count_size = (height / 20).clamp(18, 56);

    // Title in the top 60 percent, countdown in the bottom 40 percent.
    let split = rc.top + (height * 3) / 5;
    let mut title_rc = RECT {
        left: rc.left,
        top: rc.top,
        right: rc.right,
        bottom: split,
    };
    let mut count_rc = RECT {
        left: rc.left,
        top: split,
        right: rc.right,
        bottom: rc.bottom,
    };

    draw_centered(hdc, &mut title_rc, TITLE, title_size, 700);

    let remaining = REMAINING_SECS.load(Ordering::SeqCst).max(0);
    let countdown = format!("Unlocks in {} s", remaining);
    draw_centered(hdc, &mut count_rc, &countdown, count_size, 400);

    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_centered(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rc: &mut RECT,
    text: &str,
    pixel_height: i32,
    weight: i32,
) {
    let font = make_font(pixel_height, weight);
    let old = SelectObject(hdc, font.into());
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(
        hdc,
        &mut wide,
        rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
    let _ = DeleteObject(font.into());
}

unsafe fn make_font(pixel_height: i32, weight: i32) -> HFONT {
    CreateFontW(
        -pixel_height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        0,
        PCWSTR(w!("Segoe UI").0),
    )
}
