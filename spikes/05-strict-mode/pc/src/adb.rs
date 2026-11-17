//! Running adb. The commands talk to the [Adb] trait, so the unit tests can
//! run them against canned output instead of a phone.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// What one adb run gave back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Out {
    /// The exit code, or None if it was killed or ended without one.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl Out {
    /// Standard output and standard error together, the way a person sees them.
    pub fn text(&self) -> String {
        match (self.stdout.trim().is_empty(), self.stderr.trim().is_empty()) {
            (false, false) => format!("{}\n{}", self.stdout, self.stderr),
            (false, true) => self.stdout.clone(),
            (true, false) => self.stderr.clone(),
            (true, true) => String::new(),
        }
    }

    /// True when adb ended by itself with exit code 0.
    pub fn ok(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }
}

pub trait Adb {
    /// Runs adb with these arguments. The real one prints the exact line
    /// first, then runs it, and kills it after `timeout`.
    fn run(&mut self, args: &[&str], timeout: Duration) -> Result<Out, String>;

    /// Waits. The test fake does not.
    fn sleep(&mut self, d: Duration);
}

/// The line as a person would type it, starting with "adb".
pub fn shown(args: &[&str]) -> String {
    let mut s = String::from("adb");
    for a in args {
        s.push(' ');
        s.push_str(&quote_arg(a));
    }
    s
}

/// Quotes an argument for display if it holds a space or a shell character.
pub fn quote_arg(a: &str) -> String {
    let plain = !a.is_empty()
        && a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=,\\@%+".contains(c));
    if plain {
        a.to_string()
    } else {
        format!("\"{}\"", a.replace('"', "\\\""))
    }
}

/// The real adb on this PC.
pub struct RealAdb {
    exe: PathBuf,
}

impl RealAdb {
    /// Looks for adb on the PATH, then in ANDROID_HOME and ANDROID_SDK_ROOT,
    /// then in the usual Android Studio place under LOCALAPPDATA.
    pub fn find() -> Result<RealAdb, String> {
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
            tried.push(
                PathBuf::from(local)
                    .join("Android")
                    .join("Sdk")
                    .join("platform-tools")
                    .join(name),
            );
        }
        for p in tried {
            if p.is_file() {
                return Ok(RealAdb { exe: p });
            }
        }
        Err("adb was not found on the PATH, in ANDROID_HOME, in ANDROID_SDK_ROOT or in %LOCALAPPDATA%\\Android\\Sdk\\platform-tools. Install the Android SDK platform tools.".to_string())
    }

    pub fn path(&self) -> &Path {
        &self.exe
    }

    /// Starts adb's background server with no pipes attached, so that the
    /// server, which keeps running, can never hold this program's output
    /// pipes open. Does nothing if it is already running.
    pub fn start_server(&mut self) -> Result<(), String> {
        println!("> {}", shown(&["start-server"]));
        let mut child = Command::new(&self.exe)
            .arg("start-server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not run adb: {e}"))?;
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(format!("adb start-server ended with {status}")),
                Ok(None) if start.elapsed() > Duration::from_secs(30) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("adb start-server did not finish within 30 seconds".to_string());
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(format!("could not wait for adb: {e}")),
            }
        }
    }
}

impl Adb for RealAdb {
    fn run(&mut self, args: &[&str], timeout: Duration) -> Result<Out, String> {
        println!("> {}", shown(args));
        let out = run_child(&self.exe, args, timeout)?;
        if out.timed_out {
            println!("  (no answer within {} seconds: stopped)", timeout.as_secs());
        }
        Ok(out)
    }

    fn sleep(&mut self, d: Duration) {
        thread::sleep(d);
    }
}

fn run_child(exe: &Path, args: &[&str], timeout: Duration) -> Result<Out, String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run adb: {e}"))?;

    let (tx, rx) = mpsc::channel::<(bool, Vec<u8>)>();
    if let Some(mut s) = child.stdout.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            let _ = tx.send((true, b));
        });
    }
    if let Some(mut s) = child.stderr.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            let _ = tx.send((false, b));
        });
    }
    drop(tx);

    let start = Instant::now();
    let mut timed_out = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("could not wait for adb: {e}")),
        }
    };

    // The pipes close once adb is done with them. Wait a little, not forever.
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let grace_until = Instant::now() + Duration::from_secs(3);
    for _ in 0..2 {
        let left = grace_until.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((true, b)) => stdout = b,
            Ok((false, b)) => stderr = b,
            Err(_) => break,
        }
    }
    Ok(Out {
        code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_lines_as_typed() {
        assert_eq!(
            shown(&["-s", "R5CN70", "shell", "dpm", "list-owners"]),
            "adb -s R5CN70 shell dpm list-owners"
        );
        assert_eq!(
            shown(&["install", "-t", "-r", "C:\\a b\\app-debug.apk"]),
            "adb install -t -r \"C:\\a b\\app-debug.apk\""
        );
        assert_eq!(
            shown(&["shell", "am", "start", "-n", "a.b/.Main", "--es", "suspend", "com.a,com.b"]),
            "adb shell am start -n a.b/.Main --es suspend com.a,com.b"
        );
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a\"b c"), "\"a\\\"b c\"");
    }

    #[test]
    fn out_text_and_ok() {
        let o = Out {
            code: Some(0),
            stdout: "Success\n".to_string(),
            stderr: String::new(),
            timed_out: false,
        };
        assert!(o.ok());
        assert_eq!(o.text(), "Success\n");
        let e = Out {
            code: Some(255),
            stdout: "a".to_string(),
            stderr: "b".to_string(),
            timed_out: false,
        };
        assert!(!e.ok());
        assert_eq!(e.text(), "a\nb");
        let t = Out {
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
        };
        assert!(!t.ok());
        assert_eq!(t.text(), "");
    }
}
