//! Running adb. Everything the tool asks of a phone goes through the [`Adb`]
//! trait, one call at a time, so that the tests can play a phone with a
//! fake and see exactly what would cross the cable.
//!
//! The real adb ([`RealAdb`]) runs every call through one function,
//! [`run_adb`], which always has a timeout: a call that does not end in
//! time is stopped, never left hanging. Standard input is given only to the
//! one call that carries the pairing file; every other call has none.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

/// How long an ordinary call may take: 30 seconds.
pub const T_SHORT: Duration = Duration::from_secs(30);
/// How long `adb install` may take: 3 minutes.
pub const T_INSTALL: Duration = Duration::from_secs(180);
/// How long `adb uninstall` may take: 1 minute.
pub const T_UNINSTALL: Duration = Duration::from_secs(60);

/// One call of adb.
pub struct Call<'a> {
    /// The arguments after `adb`.
    pub args: Vec<String>,
    /// Bytes for adb's standard input, then closed. None: no input at all.
    pub stdin: Option<&'a [u8]>,
    /// Stopped after this long.
    pub timeout: Duration,
    /// For a call that goes on (the log): each line of its output as it
    /// comes, instead of keeping the output.
    pub on_line: Option<&'a mut dyn FnMut(&str)>,
}

impl<'a> Call<'a> {
    /// A call with these arguments and no input.
    pub fn new(args: Vec<String>, timeout: Duration) -> Call<'a> {
        Call { args, stdin: None, timeout, on_line: None }
    }
}

/// What one call gave back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    /// The exit code, or None if adb was stopped or ended without one.
    pub code: Option<i32>,
    /// Standard output, as bytes (the phone's state file comes this way).
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: String,
    /// True when the call did not end in time and was stopped.
    pub timed_out: bool,
}

impl Reply {
    /// A reply that ended with exit code 0 and this output.
    pub fn ok_with(stdout: &str) -> Reply {
        Reply { code: Some(0), stdout: stdout.as_bytes().to_vec(), ..Reply::default() }
    }

    /// A reply that ended with this exit code and these words on standard
    /// error.
    pub fn failed_with(code: i32, stderr: &str) -> Reply {
        Reply { code: Some(code), stderr: stderr.to_string(), ..Reply::default() }
    }

    /// True when adb ended by itself with exit code 0.
    pub fn ok(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }

    /// Standard output as text.
    pub fn out(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Standard output and standard error together, the way a person sees
    /// them.
    pub fn text(&self) -> String {
        let out = self.out();
        match (out.trim().is_empty(), self.stderr.trim().is_empty()) {
            (false, false) => format!("{out}\n{}", self.stderr),
            (false, true) => out,
            (true, false) => self.stderr.clone(),
            (true, true) => String::new(),
        }
    }
}

/// adb, as the tool uses it.
pub trait Adb {
    /// Finds adb and starts its server, the first time; after that, does
    /// nothing. Says which adb it is, or why there is none to use.
    fn ready(&mut self) -> Result<String, String>;

    /// Runs one call. The error is that adb could not be run at all; a call
    /// that ran and failed is a [`Reply`].
    fn run(&mut self, call: Call<'_>) -> Result<Reply, String>;

    /// Waits. The test fake does not.
    fn pause(&mut self, d: Duration);
}

/// The line as a person would type it, starting with "adb".
pub fn shown(args: &[String]) -> String {
    let mut s = String::from("adb");
    for a in args {
        s.push(' ');
        s.push_str(&quote_arg(a));
    }
    s
}

/// Quotes an argument for display when it holds a space or a shell
/// character.
pub fn quote_arg(a: &str) -> String {
    let plain = !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,\\@%+".contains(c));
    if plain { a.to_string() } else { format!("\"{}\"", a.replace('"', "\\\"")) }
}

/// Where adb is looked for: the PATH, then `platform-tools` in
/// `ANDROID_HOME` and `ANDROID_SDK_ROOT`, then Android Studio's usual
/// place under `LOCALAPPDATA`.
pub fn places_to_look() -> Vec<PathBuf> {
    let name = if cfg!(windows) { "adb.exe" } else { "adb" };
    let mut tried: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            tried.push(dir.join(name));
        }
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(home) = std::env::var_os(var) {
            tried.push(PathBuf::from(home).join("platform-tools").join(name));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        tried.push(PathBuf::from(local).join("Android").join("Sdk").join("platform-tools").join(name));
    }
    tried
}

/// The first of `places` that is a file.
pub fn first_file(places: &[PathBuf]) -> Option<PathBuf> {
    places.iter().find(|p| p.is_file()).cloned()
}

/// The message when adb is not found.
pub const ADB_NOT_FOUND: &str = "adb was not found: not on the PATH, nor in platform-tools under ANDROID_HOME or \
     ANDROID_SDK_ROOT, nor in %LOCALAPPDATA%\\Android\\Sdk\\platform-tools. Install Android's SDK platform tools, or \
     set ANDROID_HOME to the folder of the Android SDK, and run this again. Nothing was changed.";

/// The real adb on this PC.
#[derive(Debug, Default)]
pub struct RealAdb {
    exe: Option<PathBuf>,
}

impl RealAdb {
    /// An adb not looked for yet: [`Adb::ready`] finds it.
    pub fn new() -> RealAdb {
        RealAdb::default()
    }
}

impl Adb for RealAdb {
    fn ready(&mut self) -> Result<String, String> {
        if let Some(exe) = &self.exe {
            return Ok(exe.display().to_string());
        }
        let exe = first_file(&places_to_look()).ok_or_else(|| ADB_NOT_FOUND.to_string())?;
        // adb's server keeps running after the call that starts it. Started
        // here with no pipes, it can never hold this program's output open.
        let start = vec!["start-server".to_string()];
        println!("  > {}", shown(&start));
        let started = run_adb(&exe, &start, None, T_SHORT, None, true).map_err(|e| format!("{e} Nothing was changed."))?;
        if started.timed_out {
            return Err("adb's server did not start within 30 seconds. Run adb kill-server, then this again. Nothing was changed.".to_string());
        }
        if !started.ok() {
            return Err("adb's server could not be started. Run adb kill-server, then this again. Nothing was changed.".to_string());
        }
        let version = run_adb(&exe, &["version".to_string()], None, T_SHORT, None, false)
            .ok()
            .and_then(|r| r.out().lines().find_map(|l| l.trim().strip_prefix("Android Debug Bridge version ").map(str::to_string)));
        let about = match version {
            Some(v) => format!("adb {v}, {}", exe.display()),
            None => exe.display().to_string(),
        };
        self.exe = Some(exe);
        Ok(about)
    }

    fn run(&mut self, call: Call<'_>) -> Result<Reply, String> {
        let Some(exe) = self.exe.clone() else { return Err("adb was not made ready first.".to_string()) };
        println!("  > {}", shown(&call.args));
        let reply = run_adb(&exe, &call.args, call.stdin, call.timeout, call.on_line, false)?;
        if reply.timed_out {
            println!("  (no answer within {} seconds: stopped)", call.timeout.as_secs());
        }
        Ok(reply)
    }

    fn pause(&mut self, d: Duration) {
        thread::sleep(d);
    }
}

/// What the reading threads hand back.
enum Piece {
    Line(String),
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutDone,
}

/// Runs adb once: the one function every call goes through. Writes `stdin`
/// to it and closes it (or gives it no input), reads what it prints, and
/// stops it after `timeout`. With `on_line`, each line of its output goes
/// there as it comes instead of being kept. `detached` gives it no pipes at
/// all (for starting adb's server, which keeps running).
pub fn run_adb(
    exe: &Path,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
    mut on_line: Option<&mut dyn FnMut(&str)>,
    detached: bool,
) -> Result<Reply, String> {
    let mut command = Command::new(exe);
    command.args(args);
    command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    if detached {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = command.spawn().map_err(|e| format!("adb could not be run: {e}."))?;

    // The input is written on a thread of its own, so that a full pipe can
    // never hold this one up, and closed once written. The copy the thread
    // holds is wiped when it is done.
    let writer = match (stdin, child.stdin.take()) {
        (Some(bytes), Some(mut pipe)) => {
            let bytes = Zeroizing::new(bytes.to_vec());
            Some(thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
                let _ = pipe.flush();
            }))
        }
        _ => None,
    };

    let (tx, rx) = mpsc::channel::<Piece>();
    let streaming = on_line.is_some();
    let mut waiting_for = 0;
    if let Some(out) = child.stdout.take() {
        waiting_for += 1;
        let tx = tx.clone();
        thread::spawn(move || {
            if streaming {
                let mut reader = BufReader::new(out);
                let mut line = Vec::new();
                while reader.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
                    let text = String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_string();
                    let _ = tx.send(Piece::Line(text));
                    line.clear();
                }
                let _ = tx.send(Piece::OutDone);
            } else {
                let mut b = Vec::new();
                let mut out = out;
                let _ = out.read_to_end(&mut b);
                let _ = tx.send(Piece::Out(b));
            }
        });
    }
    if let Some(mut err) = child.stderr.take() {
        waiting_for += 1;
        let tx = tx.clone();
        thread::spawn(move || {
            let mut b = Vec::new();
            let _ = err.read_to_end(&mut b);
            let _ = tx.send(Piece::Err(b));
        });
    }
    drop(tx);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut take = |piece: Piece, on_line: &mut Option<&mut dyn FnMut(&str)>, waiting_for: &mut i32| match piece {
        Piece::Line(text) => {
            if let Some(f) = on_line.as_mut() {
                f(&text);
            }
        }
        Piece::Out(b) => {
            stdout = b;
            *waiting_for -= 1;
        }
        Piece::Err(b) => {
            stderr = b;
            *waiting_for -= 1;
        }
        Piece::OutDone => *waiting_for -= 1,
    };

    let start = Instant::now();
    let mut timed_out = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            Ok(None) => {
                if let Ok(piece) = rx.recv_timeout(Duration::from_millis(50)) {
                    take(piece, &mut on_line, &mut waiting_for);
                }
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("adb could not be waited for: {e}."));
            }
        }
    };

    // The pipes close once adb is done with them. Wait a little, not
    // forever.
    let grace_until = Instant::now() + Duration::from_secs(3);
    while waiting_for > 0 {
        let left = grace_until.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(piece) => take(piece, &mut on_line, &mut waiting_for),
            Err(_) => break,
        }
    }
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(Reply { code, stdout, stderr: String::from_utf8_lossy(&stderr).into_owned(), timed_out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lines_are_shown_as_typed() {
        assert_eq!(shown(&args(&["-s", "R5CN70", "shell", "-T", "pm list packages"])), "adb -s R5CN70 shell -T \"pm list packages\"");
        assert_eq!(shown(&args(&["install", "-r", "C:\\a b\\app.apk"])), "adb install -r \"C:\\a b\\app.apk\"");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a\"b c"), "\"a\\\"b c\"");
    }

    #[test]
    fn a_reply_says_whether_it_ended_well_and_what_it_said() {
        let ok = Reply::ok_with("Success\n");
        assert!(ok.ok());
        assert_eq!(ok.text(), "Success\n");
        let failed = Reply { code: Some(1), stdout: b"a".to_vec(), stderr: "b".to_string(), timed_out: false };
        assert!(!failed.ok());
        assert_eq!(failed.text(), "a\nb");
        let stopped = Reply { timed_out: true, ..Reply::default() };
        assert!(!stopped.ok());
        assert_eq!(stopped.text(), "");
    }

    #[test]
    fn adb_is_looked_for_in_the_places_named_and_none_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nothing").join("adb.exe");
        assert_eq!(first_file(std::slice::from_ref(&missing)), None);
        let real = dir.path().join("adb.exe");
        std::fs::write(&real, b"").unwrap();
        assert_eq!(first_file(&[missing, real.clone()]), Some(real));
        assert!(ADB_NOT_FOUND.contains("ANDROID_HOME") && ADB_NOT_FOUND.contains("Nothing was changed."));
    }
}
