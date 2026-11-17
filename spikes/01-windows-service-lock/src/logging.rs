//! Append timestamped lines to the spike log, and read the tail of it.

use std::fs::OpenOptions;
use std::io::Write;

use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::config::log_path;

/// Local wall clock time as "YYYY-MM-DD HH:MM:SS".
pub fn timestamp() -> String {
    // SAFETY: GetLocalTime only writes into the SYSTEMTIME we hand it.
    let t: SYSTEMTIME = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// Append one line to the log. Failures are swallowed on purpose: logging must
/// never be the reason the service misbehaves.
pub fn log(line: &str) {
    let path = log_path();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {}", timestamp(), line);
    }
}

/// Return the last `n` lines of the log, oldest first. Empty when the log is
/// missing or unreadable.
pub fn tail(n: usize) -> Vec<String> {
    let path = log_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}
