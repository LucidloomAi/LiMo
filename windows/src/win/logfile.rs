//! The service's log, in the data folder: one timestamped line per thing
//! that happened, and the end of it for `limo status`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use super::clock::timestamp;
use crate::settings::{MAX_LOG_BYTES, Places};

/// One line for the log: its time, and the text with anything that could
/// start a new line taken out.
pub(crate) fn log_line(time: &str, text: &str) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    format!("{time} {flat}\n")
}

/// Appends a line to the log, moving the log aside first when it has grown
/// past its limit. Failures are swallowed: the log must never be why the
/// service misbehaves.
pub(crate) fn append(places: &Places, text: &str) {
    let path = places.log_file();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, places.old_log_file());
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(log_line(&timestamp(), text).as_bytes());
    }
}

/// The last `n` lines of a log, oldest first.
pub(crate) fn tail(path: &Path, n: usize) -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].iter().map(|s| s.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_line_is_one_line() {
        assert_eq!(log_line("2026-11-19 10:00:00", "Started.\r\nforged"), "2026-11-19 10:00:00 Started.  forged\n");
    }
}
