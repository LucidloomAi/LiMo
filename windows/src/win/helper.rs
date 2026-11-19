//! `limo helper`: the helper process, started by the service in a signed in
//! session, born with its channel.
//!
//! Four threads besides the user interface: one reads what the service
//! says and wakes the user interface thread; one writes what the helper
//! says; one reports, once a second, how long since the last input, the
//! process of the window in front, whether something is in full screen,
//! whether a program of the session plays sound and whether a program keeps
//! the screen awake; and one listens for that sound (`playing.rs`). When the
//! channel closes, the helper ends.

use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowRect, GetWindowThreadProcessId, KillTimer, MSG,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer, TranslateMessage, WM_APP, WM_QUIT, WM_TIMER, WM_USER,
};

use super::about::about;
use super::display::PlainDisplay;
use super::drawn::{DrawnDisplay, WM_LIMO_INPUT};
use super::pipe::{Incoming, handle_from, is_pipe, set_inheritable, spawn_reader, spawn_writer};
use super::playing::{Listener, screen_kept_awake_now};
use super::{Owned, from_wide};
use crate::helper::{Chooser, Display, HelperCore, Outbox, hello, report};
use crate::protocol::{FromHelper, ToHelper, encode};

/// Why the helper shows the plain display when it is started with
/// `--plain`.
const PLAIN_ASKED: &str = "the settings file asks for the plain display";

/// Posted to the user interface thread when something came from the
/// service, or the channel closed.
const WM_LIMO_WAKE: u32 = WM_APP + 1;

/// How often the display is pumped, in milliseconds.
const PUMP_MS: u32 = 250;

/// The classes of the shell's own desktop and taskbar windows, which cover
/// a monitor without being full screen.
const SHELL_CLASSES: [&str; 3] = ["Progman", "WorkerW", "Shell_TrayWnd"];

/// True when a window covers the whole of its monitor.
pub(crate) fn covers(window: RECT, monitor: RECT) -> bool {
    window.left <= monitor.left && window.top <= monitor.top && window.right >= monitor.right && window.bottom >= monitor.bottom
}

/// The process of the window in front, and whether that window covers its
/// whole monitor.
fn front() -> (Option<u32>, bool) {
    // SAFETY: reads about the window in front; every structure written is
    // one handed to the call.
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return (None, false);
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let mut class = [0u16; 64];
        let length = GetClassNameW(window, &mut class);
        let class = from_wide(&class[..usize::try_from(length).unwrap_or(0).min(class.len())]);
        let mut rect = RECT::default();
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let full = !SHELL_CLASSES.contains(&class.as_str())
            && GetWindowRect(window, &mut rect).is_ok()
            && GetMonitorInfoW(monitor, &mut info).as_bool()
            && covers(rect, info.rcMonitor);
        ((pid != 0).then_some(pid), full)
    }
}

/// Milliseconds since the last keyboard or mouse input in this session.
fn idle_ms() -> u64 {
    let mut last = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: writes into the structure handed to it; the tick count only
    // reads a counter.
    unsafe {
        if !GetLastInputInfo(&mut last).as_bool() {
            return 0;
        }
        u64::from(GetTickCount().wrapping_sub(last.dwTime))
    }
}

/// True when Windows says a program is in full screen, or presenting.
fn shell_says_full_screen() -> bool {
    // SAFETY: returns a value and reads nothing given.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE,
        Err(_) => false,
    }
}

/// The report of this second. Whether sound plays comes from the
/// listener's own thread, so the audio service never holds the report back.
fn report_now(own_pid: u32, listener: &Listener) -> FromHelper {
    let (front_pid, covering) = front();
    let full_screen = front_pid != Some(own_pid) && (covering || shell_says_full_screen());
    report(idle_ms(), front_pid, own_pid, full_screen, listener.playing_sound(), screen_kept_awake_now())
}

/// Runs the helper. Refuses, doing nothing, without a channel. Shows the
/// drawn display, or the plain one when `plain` is set.
pub(crate) fn run(channel_handles: Option<(usize, usize)>, plain: bool) -> i32 {
    let refuse = || {
        println!("limo helper is started by the LiMo service only, with its channel. Run by hand it does nothing.");
        2
    };
    let Some((read, write)) = channel_handles else { return refuse() };
    let (read, write) = (handle_from(read), handle_from(write));
    if !is_pipe(read) || !is_pipe(write) {
        return refuse();
    }
    let (Some(read), Some(write)) = (Owned::new(read), Owned::new(write)) else { return refuse() };
    // Nothing this helper might start inherits its channel.
    let _ = set_inheritable(&read, false);
    let _ = set_inheritable(&write, false);

    // SAFETY: each call sets up this process's own user interface thread.
    let ui_thread = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Makes this thread's message queue before anything posts to it.
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let wake = move || {
        // SAFETY: posts a message with no data to this process's own thread.
        unsafe {
            let _ = PostThreadMessageW(ui_thread, WM_LIMO_WAKE, WPARAM(0), LPARAM(0));
        }
    };

    // What the helper says: framed and written by one thread.
    let (out_tx, out_rx) = channel::<FromHelper>();
    let Some(frames) = spawn_writer("limo helper writer", write, wake) else { return 1 };
    if std::thread::Builder::new()
        .name("limo helper framer".into())
        .spawn(move || {
            for message in out_rx {
                let Ok(frame) = encode(&message) else { break };
                drop(message);
                if frames.send(frame).is_err() {
                    break;
                }
            }
        })
        .is_err()
    {
        return 1;
    }

    // What the service says: read by one thread, handed to this one.
    let (in_tx, in_rx) = channel::<Incoming<ToHelper>>();
    if !spawn_reader::<ToHelper, _>("limo helper reader", read, move |incoming| {
        let sent = in_tx.send(incoming).is_ok();
        wake();
        sent
    }) {
        return 1;
    }

    let _ = out_tx.send(hello());
    let _ = out_tx.send(about());

    // The report, once a second, with the sound listened for on a thread of
    // its own, which ends with the report's.
    let reports = out_tx.clone();
    let own_pid = std::process::id();
    let _ = std::thread::Builder::new().name("limo helper report".into()).spawn(move || {
        let listener = Listener::start();
        loop {
            if reports.send(report_now(own_pid, &listener)).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    // The drawn display, with the plain one in its place at once if it
    // cannot start or fails; the plain one alone when the settings ask.
    let make_plain = PlainDisplay::new;
    let mut display = if plain {
        Chooser::plain(PLAIN_ASKED, out_tx.clone(), make_plain)
    } else {
        Chooser::drawn(DrawnDisplay::new(Outbox::new(out_tx.clone())), out_tx.clone(), make_plain)
    };
    let mut core = HelperCore::new();
    let code = message_loop(&mut display, &mut core, &in_rx, &out_tx);
    drop(display);
    code
}

/// The user interface thread's loop, until the channel closes or the
/// service says quit.
fn message_loop(display: &mut dyn Display, core: &mut HelperCore, from_service: &Receiver<Incoming<ToHelper>>, to_service: &std::sync::mpsc::Sender<FromHelper>) -> i32 {
    // SAFETY: a timer of this thread's own, killed before returning.
    let timer = unsafe { SetTimer(None, 0, PUMP_MS, None) };
    let mut message = MSG::default();
    let code = 'running: loop {
        // SAFETY: the usual message loop of this thread.
        let got = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if got.0 <= 0 || message.message == WM_QUIT {
            break 'running 0;
        }
        if message.hwnd == HWND::default() && message.message == WM_LIMO_WAKE {
            for incoming in from_service.try_iter() {
                match incoming {
                    Incoming::Message(said) => {
                        let reaction = core.on_message(said, display);
                        for reply in reaction.replies {
                            let _ = to_service.send(reply);
                        }
                        if reaction.quit {
                            break 'running 0;
                        }
                    }
                    Incoming::Broken(_) | Incoming::Closed => break 'running 0,
                }
            }
            continue;
        }
        if message.hwnd == HWND::default() && message.message == WM_TIMER {
            display.pump();
            continue;
        }
        if message.hwnd == HWND::default() && message.message == WM_LIMO_INPUT {
            display.input();
            continue;
        }
        // SAFETY: hands a message of this thread's own windows to them.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    };
    // SAFETY: kills the timer made above.
    unsafe {
        let _ = KillTimer(None, timer);
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_full_screen_only_when_it_covers_its_monitor() {
        let monitor = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert!(covers(monitor, monitor));
        assert!(covers(RECT { left: -8, top: -8, right: 1928, bottom: 1088 }, monitor));
        assert!(!covers(RECT { left: 0, top: 0, right: 1920, bottom: 1040 }, monitor));
        let second = RECT { left: 1920, top: 0, right: 3840, bottom: 1080 };
        assert!(covers(second, second));
        assert!(!covers(monitor, second));
    }

    #[test]
    fn run_by_hand_without_a_channel_it_does_nothing() {
        assert_eq!(run(None, false), 2);
        assert_eq!(run(None, true), 2);
    }
}
