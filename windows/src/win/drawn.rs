//! The drawn display: the lock screen, the time left display and its
//! notices from `limo-screens`, on the screen. Everything it decides is in
//! `crate::drawn`, and tested there; this module only makes the windows,
//! copies the pictures onto them and hands in what the child does.
//!
//! - The lock: one borderless window over each monitor, the same kind as the
//!   plain display's (topmost tool windows, in the UIAccess band when the
//!   helper has UIAccess), put back on top four times a second, with the
//!   monitors looked at again every two seconds and whenever Windows says
//!   the displays changed. The main monitor shows the lock screen, every
//!   other one says to look at the main one. Each picture is drawn at its
//!   window's size in real pixels (the helper is per monitor DPI aware) into
//!   a bitmap of its own, after every input and about once a second, and
//!   copied to the window only when it changed. A low level keyboard hook
//!   keeps every key from every program while the lock is up, and hands the
//!   keys to the lock.
//! - The time left display and a notice above it: two layered windows that
//!   take each pixel's alpha, so their corners and the space beside their
//!   hard shadows are clear. Neither is ever activated nor takes the
//!   keyboard. The time left display can be dragged, and keeps its place
//!   while the helper runs.
//!
//! Windows' callbacks (the window procedures and the hook) only note what
//! happened and wake the helper's loop; nothing is drawn inside them. Every
//! step that draws runs under `catch_unwind`: if one ever panicked, the
//! display says it failed, and the helper puts the plain display in its
//! place at once.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use limo_agent::Show;
use limo_screens::{MAX_SIDE, Picture, PixelRect, Size, scale_for};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint, BitBlt, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, GetDC, HBITMAP, HDC, HGDIOBJ, PAINTSTRUCT, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, GetKeyState, GetKeyboardLayout, ReleaseCapture, SetCapture, ToUnicodeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, HC_ACTION, HHOOK, HWND_TOPMOST, IDC_ARROW, IDC_SIZEALL,
    KBDLLHOOKSTRUCT, LoadCursorW, MA_NOACTIVATE, PostThreadMessageW, RegisterClassExW, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos, SetWindowsHookExW, ShowWindow, ULW_ALPHA,
    UnhookWindowsHookEx, UpdateLayeredWindow, WH_KEYBOARD_LL, WM_APP, WM_CAPTURECHANGED, WM_CLOSE, WM_DISPLAYCHANGE, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT, WM_SYSKEYDOWN, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};
use zeroize::Zeroize;

use super::clock;
use super::display::{Monitor, monitors};
use super::{error_text, win32_code};
use crate::drawn::{
    Anchor, Area, DrawnCore, Mode, Moment, NOTICE_SHADOW, PILL_SHADOW, area_for, copy_to_bgra, dragged, first_char,
    kept_on_screen, notice_at, pc_clock, point_from, rect_at, shadow_px, start_anchor, vk,
};
use crate::helper::{Display, MayFail, Outbox, ShowNow, Shown};
use crate::protocol::Answer;

/// Posted to the helper's own thread when a window or the keyboard hook
/// noted something for the display to take.
pub(crate) const WM_LIMO_INPUT: u32 = WM_APP + 2;

const LOCK_CLASS: PCWSTR = w!("LiMoDrawnLock");
const PILL_CLASS: PCWSTR = w!("LiMoDrawnTimeLeft");
const NOTICE_CLASS: PCWSTR = w!("LiMoDrawnNotice");
/// `RegisterClassExW` found the class already registered: no harm.
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
/// `ToUnicodeEx`: leave the keyboard's own state, such as a dead key
/// waiting, as it was.
const KEEP_KEYBOARD_STATE: u32 = 0x4;

const MONITORS_CHECKED_EVERY: Duration = Duration::from_secs(2);
/// The lock is drawn again this often even with no input, for its clock.
const REDRAW_EVERY: Duration = Duration::from_secs(1);
/// The most things noted and not yet taken; more are dropped.
const MAX_EVENTS: usize = 256;

/// Something a window or the keyboard hook noted.
#[derive(Debug, Clone, Copy)]
enum Event {
    /// A key went down or up.
    Key { vk: u32, scan: u32, down: bool },
    /// The left button went down on a lock window, at a point in it.
    Click { window: HWND, x: i32, y: i32 },
    /// The left button went down on the time left display or a notice, at a
    /// point in it, and at a point on the screen.
    Press { window: HWND, x: i32, y: i32, screen: (i32, i32) },
    /// The mouse moved with the button held, to a point on the screen.
    Drag { screen: (i32, i32) },
    /// The button went up, or the mouse was taken elsewhere.
    Release,
    /// A lock window must have its picture put on it again.
    Repaint { window: HWND },
    /// Windows says the displays changed.
    Displays,
}

thread_local! {
    static EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
    static WOKEN: Cell<bool> = const { Cell::new(false) };
}

/// Notes what happened, and wakes the helper's loop to take it. Never
/// panics: anything that cannot be kept is dropped.
fn note(event: Event) {
    let _ = EVENTS.try_with(|events| {
        if let Ok(mut events) = events.try_borrow_mut()
            && events.len() < MAX_EVENTS
        {
            events.push_back(event);
        }
    });
    let first = WOKEN.try_with(|woken| !woken.replace(true)).unwrap_or(false);
    if first {
        // SAFETY: posts a message with no data to this thread's own queue.
        unsafe {
            let _ = PostThreadMessageW(GetCurrentThreadId(), WM_LIMO_INPUT, WPARAM(0), LPARAM(0));
        }
    }
}

/// Everything noted since the last call.
fn take_events() -> VecDeque<Event> {
    let _ = WOKEN.try_with(|woken| woken.set(false));
    EVENTS
        .try_with(|events| events.try_borrow_mut().map(|mut events| std::mem::take(&mut *events)).unwrap_or_default())
        .unwrap_or_default()
}

/// The mouse pointer on the screen.
fn pointer() -> (i32, i32) {
    let mut point = POINT::default();
    // SAFETY: writes into the point handed to it.
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x, point.y)
}

/// Keeps every key from every program while a lock is up, and notes it for
/// the lock. It does nothing else, so it returns at once.
unsafe extern "system" fn keys(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if lparam.0 != 0 {
            // SAFETY: for a low level keyboard hook called with HC_ACTION,
            // lParam points to a KBDLLHOOKSTRUCT that lives for this call.
            let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let message = wparam.0 as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            note(Event::Key { vk: info.vkCode, scan: info.scanCode, down });
        }
        return LRESULT(1);
    }
    // SAFETY: hands the event on, as a hook must when it does not act.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn lock_window(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            // SAFETY: validates the window's update region; its picture is
            // put on it again by the display, which this asks for.
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(window, &mut paint);
                let _ = EndPaint(window, &paint);
            }
            note(Event::Repaint { window });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(lparam.0);
            note(Event::Click { window, x, y });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            note(Event::Displays);
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        // SAFETY: everything else is Windows' default.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe extern "system" fn layer_window(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(lparam.0);
            let screen = pointer();
            // SAFETY: takes the mouse for a window of this thread's own while
            // the button is held, so a drag goes on outside the window.
            unsafe {
                let _ = SetCapture(window);
            }
            note(Event::Press { window, x, y, screen });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            // SAFETY: only asks which window has the mouse.
            if unsafe { GetCapture() } == window {
                note(Event::Drag { screen: pointer() });
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            // SAFETY: gives back the mouse taken on the button going down.
            unsafe {
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            note(Event::Release);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            note(Event::Displays);
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        // SAFETY: everything else is Windows' default.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// A rectangle from Windows, as the drawn display's decisions take it.
fn area_of(rect: RECT) -> Area {
    Area { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
}

/// The scale of the design on a monitor.
fn scale_of(monitor: &Monitor) -> f32 {
    let area = area_of(monitor.area);
    scale_for(area.width(), area.height())
}

/// A moment now, both ways.
fn moment() -> Moment {
    Moment { wall: clock::wall_secs(), steady_ms: clock::steady_ms() }
}

/// The PC's clock now: its time zone, and the user's own 12 or 24 hours.
fn clock_now() -> limo_screens::PcClock {
    pc_clock(clock::utc_offset_secs(), &clock::time_format())
}

/// True while Caps Lock is on. The keyboard hook keeps its key from Windows
/// too while a lock is up, so it stays as it was when the lock came.
fn caps_lock_on() -> bool {
    // SAFETY: only reads a key's state.
    unsafe { GetKeyState(vk::CAPITAL as i32) & 1 != 0 }
}

/// The text the user's own keyboard layout gives for a key, with the
/// modifiers in `state`. A dead key, or a key that gives no text, gives
/// nothing; a key that gives more than one character gives its first.
fn key_text(vk: u32, scan: u32, state: &[u8; 256]) -> Option<char> {
    let mut buffer = [0u16; 8];
    // SAFETY: reads the state handed to it, writes at most the buffer's
    // length into it, and leaves the keyboard's own state as it was.
    let written = unsafe { ToUnicodeEx(vk, scan, state, &mut buffer, KEEP_KEYBOARD_STATE, Some(GetKeyboardLayout(0))) };
    let count = usize::try_from(written).ok()?.min(buffer.len());
    first_char(&buffer[..count])
}

/// A bitmap in memory that a picture is copied into, with a device context
/// to copy it on from. Its pixels are wiped when it goes, since a lock's
/// may show a code's digits.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl Surface {
    fn new(size: Size) -> Result<Surface, String> {
        let (width, height) = (size.width.clamp(1, MAX_SIDE), size.height.clamp(1, MAX_SIDE));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: i32::try_from(width).unwrap_or(1),
                // Negative: rows from the top, as the pictures are.
                biHeight: -i32::try_from(height).unwrap_or(1),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: makes a memory device context and a bitmap of this
        // thread's own, both released in Drop (or here, on failure).
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err("a memory device context could not be made".to_string());
            }
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bitmap) if !bits.is_null() => bitmap,
                Ok(bitmap) => {
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(dc);
                    return Err("a bitmap was made with no pixels".to_string());
                }
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(format!("a bitmap of {width} by {height} could not be made: {}", error_text(&e)));
                }
            };
            let old = SelectObject(dc, bitmap.into());
            Ok(Surface { dc, bitmap, old, bits: bits.cast::<u8>(), width, height })
        }
    }

    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    fn pixels(&mut self) -> &mut [u8] {
        let length = self.width as usize * self.height as usize * 4;
        // SAFETY: the bitmap's pixels are width times height times 4 bytes,
        // rows with no padding at 32 bits a pixel, alive until the bitmap is
        // deleted in Drop; `&mut self` makes this the only view of them.
        unsafe { std::slice::from_raw_parts_mut(self.bits, length) }
    }

    /// Copies a picture in; true when a pixel changed.
    fn copy(&mut self, picture: &Picture) -> bool {
        let (width, height) = (self.width, self.height);
        copy_to_bgra(picture.rgba(), picture.size(), self.pixels(), width, height)
    }

    /// Puts the bitmap on a window now.
    fn blit(&self, window: HWND) {
        let (width, height) = (i32::try_from(self.width).unwrap_or(0), i32::try_from(self.height).unwrap_or(0));
        // SAFETY: draws on a window of this thread's own through a device
        // context that is released at once.
        unsafe {
            let dc = GetDC(Some(window));
            if !dc.is_invalid() {
                let _ = BitBlt(dc, 0, 0, width, height, Some(self.dc), 0, 0, SRCCOPY);
                ReleaseDC(Some(window), dc);
            }
        }
    }

    /// Puts the bitmap on a layered window at a point on the screen, with
    /// each pixel's alpha. True when Windows took it.
    fn put_layered(&self, window: HWND, at: (i32, i32)) -> bool {
        let position = POINT { x: at.0, y: at.1 };
        let size = SIZE { cx: i32::try_from(self.width).unwrap_or(1), cy: i32::try_from(self.height).unwrap_or(1) };
        let source = POINT::default();
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        // SAFETY: every pointer handed over is to a value on this stack that
        // outlives the call; the screen's device context is released at once.
        unsafe {
            let screen = GetDC(None);
            let done = UpdateLayeredWindow(
                window,
                Some(screen),
                Some(&position as *const POINT),
                Some(&size as *const SIZE),
                Some(self.dc),
                Some(&source as *const POINT),
                COLORREF(0),
                Some(&blend as *const BLENDFUNCTION),
                ULW_ALPHA,
            )
            .is_ok();
            ReleaseDC(None, screen);
            done
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.pixels().zeroize();
        // SAFETY: puts back what the device context held, then deletes the
        // bitmap and the device context made in `new`, once.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// One monitor's lock window, with its picture.
struct LockWindow {
    window: HWND,
    area: Area,
    surface: Surface,
    shown: bool,
}

/// A layered window: the time left display, or the notice above it.
struct Layered {
    window: HWND,
    surface: Option<Surface>,
    at: Option<(i32, i32)>,
}

impl Layered {
    /// Shows a picture at a point on the screen, copied and put on the
    /// window only when it or the point changed.
    fn put(&mut self, picture: &Picture, at: (i32, i32)) -> Result<(), String> {
        let fresh = self.surface.as_ref().is_none_or(|s| s.size() != picture.size());
        if fresh {
            self.surface = Some(Surface::new(picture.size())?);
        }
        let Some(surface) = self.surface.as_mut() else { return Ok(()) };
        let changed = surface.copy(picture) || fresh;
        if changed || self.at != Some(at) {
            if !surface.put_layered(self.window, at) {
                return Err("a layered window would not take its picture".to_string());
            }
            if self.at.is_none() {
                // SAFETY: shows a window of this thread's own without
                // activating it.
                unsafe {
                    let _ = ShowWindow(self.window, SW_SHOWNOACTIVATE);
                }
            }
            self.at = Some(at);
        }
        Ok(())
    }

    fn hide(&mut self) {
        if self.at.take().is_some() {
            // SAFETY: hides a window of this thread's own.
            unsafe {
                let _ = ShowWindow(self.window, SW_HIDE);
            }
        }
    }

    fn keep_on_top(&self) {
        if self.at.is_some() {
            // SAFETY: keeps a window of this thread's own on top, without
            // activating it.
            unsafe {
                let _ = SetWindowPos(self.window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }
}

/// Registers one of the display's window classes. One already registered,
/// by a display before this one, is no harm.
fn register(instance: HINSTANCE, name: PCWSTR, procedure: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT, cursor: PCWSTR) -> Result<(), String> {
    // SAFETY: registers a class of this program's own, with a cursor of
    // Windows' own.
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hCursor: LoadCursorW(None, cursor).unwrap_or_default(),
            lpszClassName: name,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            let error = windows::core::Error::from_thread();
            if win32_code(&error) != ERROR_CLASS_ALREADY_EXISTS {
                return Err(format!("a window class could not be registered: {}", error_text(&error)));
            }
        }
    }
    Ok(())
}

/// A hidden layered window that is never activated.
fn layered(instance: HINSTANCE, class: PCWSTR, title: PCWSTR) -> Result<Layered, String> {
    // SAFETY: makes a window of this thread's own, destroyed in Drop.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            class,
            title,
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .map_err(|e| format!("a layered window could not be made: {}", error_text(&e)))?;
    Ok(Layered { window, surface: None, at: None })
}

fn destroy(window: HWND) {
    // SAFETY: destroys a window of this thread's own.
    unsafe {
        let _ = DestroyWindow(window);
    }
}

/// The drawn display. Lives on the helper's user interface thread.
pub(crate) struct DrawnDisplay {
    core: DrawnCore,
    instance: HINSTANCE,
    monitors: Vec<Monitor>,
    monitors_checked: Instant,
    locks: Vec<LockWindow>,
    hook: Option<HHOOK>,
    lock_drawn: Instant,
    pill: Layered,
    notice: Layered,
    notice_ok: Option<PixelRect>,
    /// The time left display's picture size, and how far it reaches past
    /// its card.
    pill_size: Option<(Size, i32)>,
    /// Where the time left display is now.
    placed: Option<Anchor>,
    /// Where it was dragged to, kept while the helper runs.
    dragged_to: Option<Anchor>,
    /// While it is dragged: where it was and where the pointer was pressed.
    drag: Option<(Anchor, (i32, i32))>,
    failure: Option<String>,
}

impl DrawnDisplay {
    /// The display, with its window classes, its two layered windows and a
    /// first picture of the lock drawn, so that the fonts are read before a
    /// lock is needed. Why not, when any of that fails.
    pub(crate) fn new(outbox: Outbox) -> Result<DrawnDisplay, String> {
        // SAFETY: the handle of this program's own module.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None).map(Into::into).unwrap_or_default() };
        register(instance, LOCK_CLASS, lock_window, IDC_ARROW)?;
        register(instance, PILL_CLASS, layer_window, IDC_SIZEALL)?;
        register(instance, NOTICE_CLASS, layer_window, IDC_ARROW)?;
        let pill = layered(instance, PILL_CLASS, w!("LiMo time left"))?;
        let notice = match layered(instance, NOTICE_CLASS, w!("LiMo notice")) {
            Ok(notice) => notice,
            Err(why) => {
                destroy(pill.window);
                return Err(why);
            }
        };
        let mut core = DrawnCore::new(outbox);
        core.set_clock(clock_now());
        let first = catch_unwind(AssertUnwindSafe(|| core.lock_picture(Size::new(1366, 768)).width()));
        if first.is_err() {
            destroy(pill.window);
            destroy(notice.window);
            return Err("the lock screen could not be drawn".to_string());
        }
        Ok(DrawnDisplay {
            core,
            instance,
            monitors: monitors(),
            monitors_checked: Instant::now(),
            locks: Vec::new(),
            hook: None,
            lock_drawn: Instant::now(),
            pill,
            notice,
            notice_ok: None,
            pill_size: None,
            placed: None,
            dragged_to: None,
            drag: None,
            failure: None,
        })
    }

    /// Runs one step; if it ever panics, the display has failed, and says so.
    fn guarded<T>(&mut self, step: impl FnOnce(&mut DrawnDisplay) -> T) -> Option<T> {
        if self.failure.is_some() {
            return None;
        }
        match catch_unwind(AssertUnwindSafe(|| step(self))) {
            Ok(value) => Some(value),
            Err(_) => {
                self.failure = Some("a step of the drawn display failed unexpectedly".to_string());
                None
            }
        }
    }

    fn fail(&mut self, why: String) {
        if self.failure.is_none() {
            self.failure = Some(why);
        }
    }

    // ----------------------------------------------------------------------
    // The lock
    // ----------------------------------------------------------------------

    fn lock_covers_every_monitor(&self) -> bool {
        !self.monitors.is_empty() && self.locks.len() == self.monitors.len() && self.locks.iter().all(|l| l.shown)
    }

    /// One window over each monitor, the main one first, and the keyboard
    /// hook.
    fn begin_lock(&mut self) {
        if self.locks.is_empty() {
            let mut monitors = self.monitors.clone();
            monitors.sort_by_key(|m| !m.primary);
            for monitor in monitors {
                match self.lock_window_for(&monitor) {
                    Ok(made) => self.locks.push(made),
                    Err(why) => {
                        self.fail(format!("a lock window could not be made: {why}"));
                        break;
                    }
                }
            }
        }
        if self.hook.is_none() {
            // SAFETY: a low level hook served by this thread's message loop,
            // removed in `end_lock`.
            self.hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keys), Some(self.instance), 0).ok() };
        }
    }

    fn lock_window_for(&self, monitor: &Monitor) -> Result<LockWindow, String> {
        let area = area_of(monitor.area);
        let surface = Surface::new(area.size())?;
        // SAFETY: makes a window of this thread's own, hidden until its
        // picture is on it, destroyed when the lock ends or the monitors
        // change.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                LOCK_CLASS,
                w!("LiMo"),
                WS_POPUP,
                area.left,
                area.top,
                i32::try_from(area.width()).unwrap_or(1),
                i32::try_from(area.height()).unwrap_or(1),
                None,
                None,
                Some(self.instance),
                None,
            )
        }
        .map_err(|e| error_text(&e))?;
        Ok(LockWindow { window, area, surface, shown: false })
    }

    /// Draws each monitor's picture at its window's size, and puts it on the
    /// window when it changed. A window is shown once its picture is on it.
    fn paint_lock(&mut self) {
        if self.core.mode() != Mode::Locked {
            return;
        }
        let DrawnDisplay { core, locks, .. } = self;
        for (at, lock) in locks.iter_mut().enumerate() {
            let size = lock.area.size();
            let picture = if at == 0 { core.lock_picture(size) } else { core.other_picture(size) };
            let changed = lock.surface.copy(picture);
            if !lock.shown {
                // SAFETY: shows a window of this thread's own.
                unsafe {
                    let _ = ShowWindow(lock.window, SW_SHOW);
                }
                lock.shown = true;
                lock.surface.blit(lock.window);
            } else if changed {
                lock.surface.blit(lock.window);
            }
        }
        self.lock_drawn = Instant::now();
    }

    fn raise_lock(&self) {
        // SAFETY: moves windows of this thread's own to the top, and brings
        // the main one to the front.
        unsafe {
            for lock in &self.locks {
                let _ = SetWindowPos(lock.window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            if let Some(main) = self.locks.first() {
                let _ = SetForegroundWindow(main.window);
            }
        }
    }

    /// The lock windows go, and their bitmaps are wiped; the hook stays.
    fn drop_lock_windows(&mut self) {
        for lock in self.locks.drain(..) {
            destroy(lock.window);
        }
    }

    fn end_lock(&mut self) {
        self.drop_lock_windows();
        if let Some(hook) = self.hook.take() {
            // SAFETY: removes the hook this display set.
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    }

    // ----------------------------------------------------------------------
    // The time left display and its notices
    // ----------------------------------------------------------------------

    fn work_areas(&self) -> Vec<Area> {
        self.monitors.iter().map(|m| area_of(m.work)).collect()
    }

    fn main_monitor(&self) -> Option<Monitor> {
        self.monitors.iter().find(|m| m.primary).or(self.monitors.first()).copied()
    }

    /// Puts the time left display where it belongs, and the notice above it
    /// when there is one. True when the display is on the screen.
    fn render_overlays(&mut self) -> bool {
        if self.core.mode() != Mode::TimeLeft {
            self.hide_overlays();
            return false;
        }
        let Some(main) = self.main_monitor() else {
            self.hide_overlays();
            return false;
        };
        let works = self.work_areas();
        let anchor = self.dragged_to.unwrap_or_else(|| start_anchor(area_of(main.work), scale_of(&main)));
        let on = self
            .monitors
            .iter()
            .find(|m| area_of(m.area).contains(anchor.right.saturating_sub(1), anchor.bottom.saturating_sub(1)))
            .copied()
            .unwrap_or(main);
        let scale = scale_of(&on);

        let (pill_shadow, notice_shadow) = (shadow_px(PILL_SHADOW, scale), shadow_px(NOTICE_SHADOW, scale));

        let DrawnDisplay { core, pill, notice, .. } = self;
        let Some(picture) = core.pill(scale) else {
            self.hide_overlays();
            return false;
        };
        let size = picture.size();
        let anchor = kept_on_screen(anchor, size, pill_shadow, &works);
        let rect = rect_at(anchor, size, pill_shadow);
        let put = pill.put(picture, (rect.left, rect.top));
        let work = area_for(rect, &works).unwrap_or(area_of(on.work));
        let (put_notice, ok) = match core.notice_picture(scale) {
            Some((picture, ok)) => {
                let at = notice_at(rect, pill_shadow, picture.size(), notice_shadow, work, scale);
                (notice.put(picture, at), Some(ok))
            }
            None => {
                notice.hide();
                (Ok(()), None)
            }
        };
        self.notice_ok = ok;
        self.pill_size = Some((size, pill_shadow));
        self.placed = Some(anchor);
        if self.dragged_to.is_some() {
            self.dragged_to = Some(anchor);
        }
        match put.and(put_notice) {
            Ok(()) => true,
            Err(why) => {
                self.fail(why);
                false
            }
        }
    }

    fn hide_overlays(&mut self) {
        self.pill.hide();
        self.notice.hide();
        self.notice_ok = None;
        if self.drag.take().is_some() {
            // SAFETY: gives back the mouse taken for a drag.
            unsafe {
                let _ = ReleaseCapture();
            }
        }
    }

    // ----------------------------------------------------------------------
    // What happens
    // ----------------------------------------------------------------------

    /// Looks at the monitors again when it is time, or at once when
    /// `now`, and covers or places anew for any change.
    fn check_monitors(&mut self, now: bool) {
        if !now && self.monitors_checked.elapsed() < MONITORS_CHECKED_EVERY {
            return;
        }
        self.monitors_checked = Instant::now();
        let found = monitors();
        if found == self.monitors {
            return;
        }
        self.monitors = found;
        match self.core.mode() {
            Mode::Locked => {
                // Every monitor covered again, as they are now, before the
                // old windows go, so nothing is uncovered between; the hook
                // stays.
                let old = std::mem::take(&mut self.locks);
                self.begin_lock();
                self.paint_lock();
                self.raise_lock();
                for lock in old {
                    destroy(lock.window);
                }
            }
            Mode::TimeLeft => {
                self.render_overlays();
            }
            Mode::Nothing => {}
        }
    }

    fn put_show(&mut self, now: &ShowNow<'_>) -> Shown {
        self.core.set_clock(clock_now());
        let warning = matches!(now.show, Show::TimeLeft { warning: Some(_), .. });
        match self.core.show(now, moment()) {
            Mode::Locked => {
                self.hide_overlays();
                self.begin_lock();
                self.paint_lock();
                self.raise_lock();
                Shown { lock_on_screen: self.lock_covers_every_monitor(), warning_on_screen: false }
            }
            Mode::TimeLeft => {
                self.end_lock();
                let up = self.render_overlays();
                Shown { lock_on_screen: false, warning_on_screen: up && warning }
            }
            Mode::Nothing => {
                self.end_lock();
                self.hide_overlays();
                Shown::default()
            }
        }
    }

    fn take_input(&mut self) {
        let events = take_events();
        if events.is_empty() {
            return;
        }
        let (mut lock_changed, mut overlays_changed) = (false, false);
        for event in events {
            match event {
                Event::Key { vk, scan, down } => {
                    let text = if down && self.core.text_wanted() {
                        key_text(vk, scan, &self.core.keyboard_state(caps_lock_on()))
                    } else {
                        None
                    };
                    self.core.key(vk, down, text);
                    lock_changed |= down;
                }
                Event::Click { window, x, y } => {
                    if self.locks.first().is_some_and(|main| main.window == window) {
                        self.core.click(x as f32, y as f32);
                        lock_changed = true;
                    }
                }
                Event::Press { window, x, y, screen } => {
                    if window == self.pill.window {
                        self.drag = self.placed.map(|anchor| (anchor, screen));
                    } else if window == self.notice.window
                        && let Some(ok) = self.notice_ok
                        && self.core.notice_click(ok, x as f32, y as f32)
                    {
                        overlays_changed = true;
                    }
                }
                Event::Drag { screen } => {
                    if let (Some((from, pressed)), Some((size, shadow))) = (self.drag, self.pill_size) {
                        self.dragged_to = Some(dragged(from, pressed, screen, size, shadow, &self.work_areas()));
                        overlays_changed = true;
                    }
                }
                Event::Release => self.drag = None,
                Event::Repaint { window } => {
                    if let Some(lock) = self.locks.iter().find(|lock| lock.window == window) {
                        lock.surface.blit(lock.window);
                    }
                }
                Event::Displays => self.check_monitors(true),
            }
        }
        if lock_changed {
            self.paint_lock();
        }
        if overlays_changed {
            self.render_overlays();
        }
    }

    fn tick(&mut self) {
        self.take_input();
        self.check_monitors(false);
        self.core.tick(moment());
        match self.core.mode() {
            Mode::Locked => {
                if self.lock_drawn.elapsed() >= REDRAW_EVERY {
                    self.core.set_clock(clock_now());
                    self.paint_lock();
                }
                self.raise_lock();
            }
            Mode::TimeLeft => {
                // The last minute counts down, and notices go by themselves.
                self.render_overlays();
                self.pill.keep_on_top();
                self.notice.keep_on_top();
            }
            Mode::Nothing => {}
        }
    }
}

impl Display for DrawnDisplay {
    fn show(&mut self, now: &ShowNow<'_>) -> Shown {
        self.guarded(|display| display.put_show(now)).unwrap_or_default()
    }

    fn notice(&mut self, text: &str, program: Option<&str>) {
        self.guarded(|display| {
            display.core.notice(text, program, moment());
            display.render_overlays();
        });
    }

    fn answer(&mut self, ask: u32, answer: &Answer) {
        self.guarded(|display| {
            if display.core.answer(ask, answer) {
                display.paint_lock();
            }
        });
    }

    fn pump(&mut self) {
        self.guarded(DrawnDisplay::tick);
    }

    fn input(&mut self) {
        self.guarded(DrawnDisplay::take_input);
    }
}

impl MayFail for DrawnDisplay {
    fn failure(&self) -> Option<String> {
        self.failure.clone()
    }
}

impl Drop for DrawnDisplay {
    fn drop(&mut self) {
        self.end_lock();
        self.hide_overlays();
        destroy(self.pill.window);
        destroy(self.notice.window);
    }
}
