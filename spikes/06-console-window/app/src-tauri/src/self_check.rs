//! Self check mode: `limo-console-proof.exe --self-check <log file> [--hold-ms N]`.
//!
//! The page reports what it drew through `self_check_report`; each line goes
//! to the log with the milliseconds since the process started. When every step
//! has been reported the program exits with code 0 (after `--hold-ms`, so the
//! memory of the open window can be read from outside). If a step is missing
//! after 20 seconds it writes which ones and exits with code 1.
//!
//! Outside self check mode none of this is reachable: the window is not given
//! the permission for the two self check commands (see main.rs), and the
//! commands themselves refuse to work without a `SelfCheck`.

use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// Every step the page must report, in the order it reports them.
pub const STEPS: &[&str] = &[
    "js_start",
    "today_rendered",
    "rust_answer",
    "give_15",
    "child_route",
    "lock_now",
    "reload_requested",
    "after_reload",
];

const DEADLINE: Duration = Duration::from_secs(20);

pub struct SelfCheck {
    start: Instant,
    hold: Duration,
    inner: Mutex<Inner>,
}

struct Inner {
    file: File,
    reported: Vec<String>,
    finished: bool,
}

/// Held as Tauri state in both modes; `None` outside self check mode.
pub struct SelfCheckSlot(pub Option<Arc<SelfCheck>>);

#[derive(Serialize)]
pub struct SelfCheckState {
    active: bool,
    reported: Vec<String>,
}

impl SelfCheck {
    pub fn create(path: &Path, start: Instant, hold: Duration) -> std::io::Result<Self> {
        Ok(SelfCheck { start, hold, inner: Mutex::new(Inner { file: File::create(path)?, reported: Vec::new(), finished: false }) })
    }

    fn ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    fn write(inner: &mut Inner, ms: u128, key: &str, text: &str) {
        // One line per event, whatever the page sends.
        let mut text: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        if text.len() > 4000 {
            let mut cut = 4000;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str(" (cut)");
        }
        let _ = writeln!(inner.file, "{ms:>6} ms  {key}: {text}");
        let _ = inner.file.flush();
    }

    /// A line from the Rust side.
    pub fn log(&self, key: &str, text: &str) {
        let ms = self.ms();
        if let Ok(mut inner) = self.inner.lock() {
            Self::write(&mut inner, ms, key, text);
        }
    }

    pub fn state(&self) -> SelfCheckState {
        let reported = self.inner.lock().map(|i| i.reported.clone()).unwrap_or_default();
        SelfCheckState { active: true, reported }
    }

    /// A line from the page.
    pub fn report<R: Runtime>(&self, app: &AppHandle<R>, key: &str, text: &str) -> Result<(), String> {
        if key != "error" && !STEPS.contains(&key) {
            return Err(format!("unknown self check step {key}"));
        }
        let ms = self.ms();
        let mut inner = self.inner.lock().map_err(|_| "self check state is poisoned".to_string())?;
        Self::write(&mut inner, ms, &format!("page {key}"), text);
        if STEPS.contains(&key) && !inner.reported.iter().any(|k| k == key) {
            inner.reported.push(key.to_string());
        }
        let all = STEPS.iter().all(|s| inner.reported.iter().any(|k| k == s));
        if all && !inner.finished {
            inner.finished = true;
            Self::write(&mut inner, ms, "done", &format!("all {} steps reported, exit code 0", STEPS.len()));
            let app = app.clone();
            let hold = self.hold;
            thread::spawn(move || {
                thread::sleep(hold);
                app.exit(0);
            });
        }
        Ok(())
    }

    /// Called when the event loop ends. If the run is not finished (someone
    /// closed the window early), writes what is missing and returns false.
    pub fn finish_at_exit(&self) -> bool {
        let ms = self.ms();
        let Ok(mut inner) = self.inner.lock() else { return false };
        if inner.finished {
            return true;
        }
        inner.finished = true;
        let missing: Vec<&str> = STEPS.iter().copied().filter(|s| !inner.reported.iter().any(|k| k == s)).collect();
        Self::write(&mut inner, ms, "closed", &format!("window closed early, missing: {}; exit code 1", missing.join(", ")));
        false
    }

    /// Starts the 20 second deadline.
    pub fn start_deadline<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>) {
        let me = Arc::clone(self);
        thread::spawn(move || {
            let left = DEADLINE.saturating_sub(me.start.elapsed());
            thread::sleep(left);
            let ms = me.ms();
            {
                let Ok(mut inner) = me.inner.lock() else { return };
                if inner.finished {
                    return;
                }
                inner.finished = true;
                let missing: Vec<&str> =
                    STEPS.iter().copied().filter(|s| !inner.reported.iter().any(|k| k == s)).collect();
                Self::write(&mut inner, ms, "timeout", &format!("missing after 20 s: {}; exit code 1", missing.join(", ")));
            }
            app.exit(1);
            // If the event loop does not end by itself, end the process.
            thread::sleep(Duration::from_secs(3));
            std::process::exit(1);
        });
    }
}
