//! Self check mode: `limo-console.exe --self-check <log file> [options]`.
//!
//! The page drives its own sign in screen and reports each step through the
//! console's one command; each line goes to the log with the milliseconds
//! since the process started. When every step is in, the program waits
//! `--hold-ms` (for the measurement of what WebView2 contacts) and exits with
//! code 0. A step missing after the deadline writes which ones and exits
//! with code 1; so does a window closed early. Whatever happens, the
//! program ends before [`HARD_LIMIT`]: the window never stays a minute.
//!
//! Outside self check mode none of this is reachable: the page's driver
//! starts only when this mode put its mark on the page, and the command's
//! self check requests are refused without a `SelfCheck`.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

/// The steps the page reports, in order, for a whole run.
pub const STEPS: &[&str] = &[
    "js_start",
    "sign_in_drawn",
    "wrong_password",
    "signed_in",
    "ready_for_keys",
    "keys",
    "child_account",
    "not_the_service",
];

/// The steps of a control run (keys left on): the run ends after the key
/// probe, since F5 is meant to reload the page there.
pub const CONTROL_STEPS: &[&str] = &["js_start", "sign_in_drawn", "wrong_password", "signed_in", "ready_for_keys"];

/// Reports that are not steps: the page saw something, or is about to
/// reload itself.
const NOTES: &[&str] = &["error", "reload_requested", "saw"];

/// How long the steps may take.
const DEADLINE: Duration = Duration::from_secs(25);

/// The process ends before this, whatever happens.
pub const HARD_LIMIT: Duration = Duration::from_secs(57);

/// Where the key probe is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Probe {
    /// Not asked for.
    #[default]
    None,
    /// Asked for; waiting for the page.
    Waiting,
    /// Running or finished; the page may report.
    Done,
}

pub struct SelfCheck {
    start: Instant,
    hold: Duration,
    steps: &'static [&'static str],
    inner: Mutex<Inner>,
}

struct Inner {
    file: File,
    reported: Vec<String>,
    finished: bool,
    probe: Probe,
    page_loads: u32,
}

impl SelfCheck {
    pub fn create(path: &Path, start: Instant, hold: Duration, control: bool, probe: bool) -> std::io::Result<SelfCheck> {
        Ok(SelfCheck {
            start,
            hold,
            steps: if control { CONTROL_STEPS } else { STEPS },
            inner: Mutex::new(Inner {
                file: File::create(path)?,
                reported: Vec::new(),
                finished: false,
                probe: if probe { Probe::Waiting } else { Probe::None },
                page_loads: 0,
            }),
        })
    }

    /// Milliseconds since the process started.
    pub fn ms(&self) -> u128 {
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

    /// A page load finished; counts them, for F5.
    pub fn page_loaded(&self, url: &str) {
        let ms = self.ms();
        if let Ok(mut inner) = self.inner.lock() {
            inner.page_loads += 1;
            let n = inner.page_loads;
            Self::write(&mut inner, ms, "page_load_finished", &format!("{url} (load {n})"));
        }
    }

    /// How many page loads have finished.
    pub fn page_loads(&self) -> u32 {
        self.inner.lock().map(|i| i.page_loads).unwrap_or(0)
    }

    /// What the page asks for: the steps so far, the stand in's password,
    /// and the key probe's state.
    pub fn state(&self, password: &str) -> Value {
        let (reported, probe) = self.inner.lock().map(|i| (i.reported.clone(), i.probe)).unwrap_or_default();
        let probe = match probe {
            Probe::None => "none",
            Probe::Waiting => "waiting",
            Probe::Done => "done",
        };
        json!({ "active": true, "reported": reported, "password": password, "probe": probe })
    }

    /// A line from the page. True when this was the step that starts the
    /// key probe.
    pub fn report<R: Runtime>(&self, app: &AppHandle<R>, key: &str, text: &str) -> Result<bool, String> {
        if !self.steps.contains(&key) && !NOTES.contains(&key) && !STEPS.contains(&key) {
            return Err(format!("unknown self check step {key}"));
        }
        let ms = self.ms();
        let mut inner = self.inner.lock().map_err(|_| "self check state is poisoned".to_string())?;
        Self::write(&mut inner, ms, &format!("page {key}"), text);
        if STEPS.contains(&key) && !inner.reported.iter().any(|k| k == key) {
            inner.reported.push(key.to_string());
        }
        let starts_probe = key == "ready_for_keys" && inner.probe == Probe::Waiting;
        // While the probe waits the run is not done: see `probe_finished`.
        self.finish_if_all(app, &mut inner, ms);
        Ok(starts_probe)
    }

    /// The key probe has finished.
    pub fn probe_finished<R: Runtime>(&self, app: &AppHandle<R>) {
        let ms = self.ms();
        if let Ok(mut inner) = self.inner.lock() {
            inner.probe = Probe::Done;
            self.finish_if_all(app, &mut inner, ms);
        }
    }

    fn finish_if_all<R: Runtime>(&self, app: &AppHandle<R>, inner: &mut Inner, ms: u128) {
        let all = self.steps.iter().all(|s| inner.reported.iter().any(|k| k == s));
        let probe_over = inner.probe != Probe::Waiting;
        if all && probe_over && !inner.finished {
            inner.finished = true;
            Self::write(inner, ms, "done", &format!("all {} steps reported, exit code 0", self.steps.len()));
            let app = app.clone();
            // The hold never takes the run past the hard limit.
            let hold = self.hold.min(HARD_LIMIT.saturating_sub(self.start.elapsed() + Duration::from_secs(2)));
            thread::spawn(move || {
                thread::sleep(hold);
                app.exit(0);
            });
        }
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
        let missing: Vec<&str> = self.steps.iter().copied().filter(|s| !inner.reported.iter().any(|k| k == s)).collect();
        Self::write(&mut inner, ms, "closed", &format!("window closed early, missing: {}; exit code 1", missing.join(", ")));
        false
    }

    /// Starts the deadline, and the hard limit behind it.
    pub fn start_deadline<R: Runtime>(self: &std::sync::Arc<Self>, app: AppHandle<R>) {
        let me = std::sync::Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(DEADLINE.saturating_sub(me.start.elapsed()));
            let ms = me.ms();
            let late = {
                let Ok(mut inner) = me.inner.lock() else { return };
                if inner.finished {
                    false
                } else {
                    inner.finished = true;
                    let missing: Vec<&str> =
                        me.steps.iter().copied().filter(|s| !inner.reported.iter().any(|k| k == s)).collect();
                    Self::write(&mut inner, ms, "timeout", &format!("missing after {} s: {}; exit code 1", DEADLINE.as_secs(), missing.join(", ")));
                    true
                }
            };
            if late {
                app.exit(1);
                thread::sleep(Duration::from_secs(3));
                std::process::exit(1);
            }
            // Finished in time: the hard limit still stands behind the hold.
            thread::sleep(HARD_LIMIT.saturating_sub(me.start.elapsed()));
            me.log("hard_limit", "the run reached its limit and ends now");
            std::process::exit(0);
        });
    }
}
