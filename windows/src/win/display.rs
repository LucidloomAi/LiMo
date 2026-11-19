//! The plain display: a [`Display`] with no styling, ported from the first
//! spike's overlay, until the real lock screen and time left display are
//! drawn to the agreed design.
//!
//! - A lock: one borderless window over each monitor, above every other
//!   window (in the UIAccess band, when the helper has UIAccess), put back on
//!   top four times a second, with the lock screen's title, the child's
//!   name, its message and when it opens again. A low level keyboard hook
//!   keeps every key from every program while the lock is up. The mouse
//!   still moves, over LiMo's own windows. No ways out yet.
//! - The time left: one small window by the clock, above every other, never
//!   activated, with the time left, the end of a guardian's unlock, a
//!   warning for 30 seconds and a notice for 10.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use limo_agent::Show;
use limo_core::WarningCause;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DT_CALCRECT, DT_CENTER,
    DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, DeleteObject, DrawTextW, EndPaint, EnumDisplayMonitors, FillRect, GetMonitorInfoW, HDC,
    HMONITOR, InvalidateRect, MONITORINFO, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HC_ACTION, HHOOK, HWND_TOPMOST, IDC_ARROW,
    LoadCursorW, MA_NOACTIVATE, MONITORINFOF_PRIMARY, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos, SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_CLOSE, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSEXW, WS_BORDER, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::{BOOL, PCWSTR, w};

use super::clock::{local_moment, wall_secs};
use crate::helper::{Display, Outbox, ShowNow, Shown};
use crate::protocol::Answer;
use crate::text;

const BACKGROUND: COLORREF = COLORREF(0x0014_1414);
const INK: COLORREF = COLORREF(0x00ED_EDED);
const LOCK_CLASS: PCWSTR = w!("LiMoPlainLock");
const LEFT_CLASS: PCWSTR = w!("LiMoPlainTimeLeft");
const WARNING_SHOWN_FOR: Duration = Duration::from_secs(30);
const NOTICE_SHOWN_FOR: Duration = Duration::from_secs(10);
const MONITORS_CHECKED_EVERY: Duration = Duration::from_secs(2);
const LEFT_WIDTH: i32 = 440;
const LEFT_MARGIN: i32 = 16;

/// How big a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Size {
    Title,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    text: String,
    size: Size,
}

impl Line {
    fn title(text: &str) -> Line {
        Line { text: text.to_string(), size: Size::Title }
    }

    fn body(text: &str) -> Line {
        Line { text: text.to_string(), size: Size::Body }
    }
}

thread_local! {
    static LOCK_LINES: RefCell<Vec<Line>> = const { RefCell::new(Vec::new()) };
    static LEFT_LINES: RefCell<Vec<Line>> = const { RefCell::new(Vec::new()) };
}

/// One monitor: its whole area, the part not under the taskbar, and
/// whether it is the main one.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Monitor {
    area: RECT,
    work: RECT,
    primary: bool,
}

fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(monitor: HMONITOR, _dc: HDC, _clip: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the Vec handed to EnumDisplayMonitors below,
        // which calls this on the same thread before it returns.
        unsafe {
            let found = &mut *(data.0 as *mut Vec<Monitor>);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                found.push(Monitor { area: info.rcMonitor, work: info.rcWork, primary: info.dwFlags & MONITORINFOF_PRIMARY != 0 });
            }
        }
        BOOL(1)
    }
    let mut found: Vec<Monitor> = Vec::new();
    // SAFETY: the callback writes only into `found`, during this call.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut found as *mut Vec<Monitor> as isize));
    }
    found
}

/// Keeps every key from every program while the lock is up.
unsafe extern "system" fn swallow_keys(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        return LRESULT(1);
    }
    // SAFETY: hands the event on, as a hook must when it does not act.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn font(height: i32, weight: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    // SAFETY: makes a font object; the caller deletes it.
    unsafe {
        CreateFontW(
            -height,
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
            w!("Segoe UI"),
        )
    }
}

/// Paints lines on a window, centred as a block for the lock, from the top
/// left for the time left.
fn paint(window: HWND, lines: &[Line], centred: bool) {
    // SAFETY: painting a window of this thread's own, between BeginPaint and
    // EndPaint; every object made is deleted.
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut paint);
        let mut client = RECT::default();
        let _ = GetClientRect(window, &mut client);
        let brush = CreateSolidBrush(BACKGROUND);
        FillRect(dc, &client, brush);
        let _ = DeleteObject(brush.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, INK);
        let height = (client.bottom - client.top).max(1);
        let (title_px, body_px) = if centred { ((height / 12).clamp(28, 80), (height / 28).clamp(16, 36)) } else { (20, 18) };
        let margin = if centred { (client.right - client.left) / 10 } else { 12 };
        let align = if centred { DT_CENTER } else { DT_LEFT };
        let mut placed = Vec::new();
        let mut total = 0;
        for line in lines {
            let (px, weight) = match line.size {
                Size::Title => (title_px, 700),
                Size::Body => (body_px, 400),
            };
            let made = font(px, weight);
            let old = SelectObject(dc, made.into());
            let mut text: Vec<u16> = line.text.encode_utf16().collect();
            let mut room = RECT { left: client.left + margin, top: 0, right: client.right - margin, bottom: 0 };
            DrawTextW(dc, &mut text, &mut room, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | align);
            SelectObject(dc, old);
            let _ = DeleteObject(made.into());
            let line_height = room.bottom - room.top;
            placed.push((line, px, weight, line_height));
            total += line_height + px / 2;
        }
        let mut y = if centred { client.top + (height - total).max(0) / 2 } else { client.top + 8 };
        for (line, px, weight, line_height) in placed {
            let made = font(px, weight);
            let old = SelectObject(dc, made.into());
            let mut text: Vec<u16> = line.text.encode_utf16().collect();
            let mut room = RECT { left: client.left + margin, top: y, right: client.right - margin, bottom: y + line_height };
            DrawTextW(dc, &mut text, &mut room, DT_WORDBREAK | DT_NOPREFIX | align);
            SelectObject(dc, old);
            let _ = DeleteObject(made.into());
            y += line_height + px / 2;
        }
        let _ = EndPaint(window, &paint);
    }
}

unsafe extern "system" fn lock_window(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            LOCK_LINES.with(|lines| paint(window, &lines.borrow(), true));
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        // SAFETY: everything else is Windows' default.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe extern "system" fn left_window(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            LEFT_LINES.with(|lines| paint(window, &lines.borrow(), false));
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_CLOSE => LRESULT(0),
        // SAFETY: everything else is Windows' default.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Nothing,
    TimeLeft,
    Locked,
}

/// The plain display. Lives on the helper's user interface thread.
pub(crate) struct PlainDisplay {
    _outbox: Outbox,
    instance: HINSTANCE,
    mode: Mode,
    monitors: Vec<Monitor>,
    monitors_checked: Instant,
    lock_windows: Vec<HWND>,
    hook: Option<HHOOK>,
    left: Option<HWND>,
    left_text: String,
    unlock_text: Option<String>,
    warning: Option<(String, Instant)>,
    warning_key: Option<(WarningCause, u32, i64)>,
    notice: Option<(String, Instant)>,
}

impl PlainDisplay {
    /// The display, with the outbox the ways out will use once they are
    /// drawn.
    pub(crate) fn new(outbox: Outbox) -> PlainDisplay {
        // SAFETY: registers this program's two window classes.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None).map(Into::into).unwrap_or_default() };
        // SAFETY: as above; a class already registered is no harm.
        unsafe {
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
            for (name, procedure) in [
                (LOCK_CLASS, lock_window as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
                (LEFT_CLASS, left_window),
            ] {
                let class = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(procedure),
                    hInstance: instance,
                    hCursor: cursor,
                    lpszClassName: name,
                    ..Default::default()
                };
                RegisterClassExW(&class);
            }
        }
        PlainDisplay {
            _outbox: outbox,
            instance,
            mode: Mode::Nothing,
            monitors: monitors(),
            monitors_checked: Instant::now(),
            lock_windows: Vec::new(),
            hook: None,
            left: None,
            left_text: String::new(),
            unlock_text: None,
            warning: None,
            warning_key: None,
            notice: None,
        }
    }

    fn lock_covers_every_monitor(&self) -> bool {
        !self.monitors.is_empty() && self.lock_windows.len() == self.monitors.len()
    }

    fn begin_lock(&mut self) {
        if self.lock_windows.is_empty() {
            for monitor in self.monitors.clone() {
                let RECT { left, top, right, bottom } = monitor.area;
                // SAFETY: makes a window of this thread's own.
                let made = unsafe {
                    CreateWindowExW(
                        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                        LOCK_CLASS,
                        w!("LiMo"),
                        WS_POPUP | WS_VISIBLE,
                        left,
                        top,
                        right - left,
                        bottom - top,
                        None,
                        None,
                        Some(self.instance),
                        None,
                    )
                };
                if let Ok(window) = made {
                    if monitor.primary {
                        self.lock_windows.insert(0, window);
                    } else {
                        self.lock_windows.push(window);
                    }
                }
            }
        }
        if self.hook.is_none() {
            // SAFETY: a low level hook served by this thread's message loop.
            self.hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(swallow_keys), Some(self.instance), 0).ok() };
        }
        self.raise_lock();
    }

    fn raise_lock(&self) {
        // SAFETY: moves and repaints windows of this thread's own.
        unsafe {
            for window in &self.lock_windows {
                let _ = SetWindowPos(*window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            if let Some(first) = self.lock_windows.first() {
                let _ = SetForegroundWindow(*first);
            }
        }
    }

    fn end_lock(&mut self) {
        // SAFETY: destroys windows and a hook of this thread's own.
        unsafe {
            for window in self.lock_windows.drain(..) {
                let _ = DestroyWindow(window);
            }
            if let Some(hook) = self.hook.take() {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    }

    fn hide_left(&mut self) {
        if let Some(window) = self.left {
            // SAFETY: hides a window of this thread's own.
            unsafe {
                let _ = ShowWindow(window, SW_HIDE);
            }
        }
    }

    /// Shows the time left window with its lines now; true when it is up.
    fn render_left(&mut self) -> bool {
        let now = Instant::now();
        self.warning = self.warning.take().filter(|(_, until)| now < *until);
        self.notice = self.notice.take().filter(|(_, until)| now < *until);
        let mut lines = vec![Line::title(&self.left_text)];
        lines.extend(self.unlock_text.as_deref().map(Line::body));
        lines.extend(self.warning.as_ref().map(|(text, _)| Line::body(text)));
        lines.extend(self.notice.as_ref().map(|(text, _)| Line::body(text)));
        let height = 24 + 34 * i32::try_from(lines.len()).unwrap_or(4) + if lines.len() > 1 { 24 } else { 0 };
        LEFT_LINES.with(|kept| *kept.borrow_mut() = lines);
        let Some(work) = self.monitors.iter().find(|m| m.primary).or(self.monitors.first()).map(|m| m.work) else {
            return false;
        };
        let x = work.right - LEFT_WIDTH - LEFT_MARGIN;
        let y = work.bottom - height - LEFT_MARGIN;
        // SAFETY: makes, places and repaints a window of this thread's own.
        unsafe {
            if self.left.is_none() {
                self.left = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    LEFT_CLASS,
                    w!("LiMo time left"),
                    WS_POPUP | WS_BORDER,
                    x,
                    y,
                    LEFT_WIDTH,
                    height,
                    None,
                    None,
                    Some(self.instance),
                    None,
                )
                .ok();
            }
            let Some(window) = self.left else { return false };
            let _ = SetWindowPos(window, Some(HWND_TOPMOST), x, y, LEFT_WIDTH, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(window), None, false);
        }
        true
    }
}

impl Display for PlainDisplay {
    fn show(&mut self, now: &ShowNow<'_>) -> Shown {
        match now.show {
            Show::Nothing => {
                self.end_lock();
                self.hide_left();
                self.mode = Mode::Nothing;
                Shown::default()
            }
            Show::TimeLeft { seconds_left, counting, warning, unlocked_until } => {
                self.end_lock();
                self.mode = Mode::TimeLeft;
                self.left_text = text::time_left(*seconds_left, *counting);
                let today = local_moment(wall_secs());
                self.unlock_text = unlocked_until
                    .and_then(local_moment)
                    .map(|at| match today {
                        Some(today) => format!("Unlocked by a guardian until {}.", text::when(today, at).trim_start_matches("at ")),
                        None => format!("Unlocked by a guardian until {}.", text::clock(at)),
                    });
                if let Some(warning) = warning {
                    let key = (warning.cause, warning.minutes, warning.lock_at);
                    if self.warning_key != Some(key) {
                        self.warning_key = Some(key);
                        self.warning = Some((warning.message.clone(), Instant::now() + WARNING_SHOWN_FOR));
                    }
                }
                let up = self.render_left();
                Shown { lock_on_screen: false, warning_on_screen: up && warning.is_some() }
            }
            Show::Locked { screen } => {
                self.hide_left();
                self.mode = Mode::Locked;
                let mut lines = vec![Line::title(&screen.title)];
                lines.extend(now.child_name.map(Line::body));
                lines.push(Line::body(&screen.message));
                if let (Some(until), Some(today)) = (screen.until, local_moment(wall_secs()))
                    && let Some(at) = local_moment(until)
                {
                    lines.push(Line::body(&text::opens_again(today, at)));
                }
                let changed = LOCK_LINES.with(|kept| {
                    let mut kept = kept.borrow_mut();
                    let changed = *kept != lines;
                    *kept = lines;
                    changed
                });
                self.begin_lock();
                if changed {
                    for window in &self.lock_windows {
                        // SAFETY: repaints a window of this thread's own.
                        unsafe {
                            let _ = InvalidateRect(Some(*window), None, false);
                        }
                    }
                }
                Shown { lock_on_screen: self.lock_covers_every_monitor(), warning_on_screen: false }
            }
        }
    }

    fn notice(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now() + NOTICE_SHOWN_FOR));
        if self.mode == Mode::TimeLeft {
            self.render_left();
        }
    }

    fn answer(&mut self, _ask: u32, _answer: &Answer) {
        // The plain display offers no ways out, so it asks nothing and has
        // no answer to show.
    }

    fn pump(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.monitors_checked) >= MONITORS_CHECKED_EVERY {
            self.monitors_checked = now;
            let found = monitors();
            if found != self.monitors {
                self.monitors = found;
                if self.mode == Mode::Locked {
                    // Every monitor covered again, as they are now.
                    let hook = self.hook.take();
                    // SAFETY: destroys windows of this thread's own.
                    unsafe {
                        for window in self.lock_windows.drain(..) {
                            let _ = DestroyWindow(window);
                        }
                    }
                    self.hook = hook;
                    self.begin_lock();
                }
            }
        }
        match self.mode {
            Mode::Locked => self.raise_lock(),
            Mode::TimeLeft => {
                let fading = self.warning.as_ref().is_some_and(|(_, until)| now >= *until)
                    || self.notice.as_ref().is_some_and(|(_, until)| now >= *until);
                if fading {
                    self.render_left();
                } else if let Some(window) = self.left {
                    // SAFETY: keeps a window of this thread's own on top.
                    unsafe {
                        let _ = SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                }
            }
            Mode::Nothing => {}
        }
    }
}

impl Drop for PlainDisplay {
    fn drop(&mut self) {
        self.end_lock();
        if let Some(window) = self.left.take() {
            // SAFETY: destroys a window of this thread's own.
            unsafe {
                let _ = DestroyWindow(window);
            }
        }
    }
}
