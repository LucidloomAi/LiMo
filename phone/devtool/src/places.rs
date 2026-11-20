//! Where the test family lives on this PC, and the tool's own log.
//!
//! **Not the real family's file.** The Windows service keeps the family's
//! file in its own data folder, open to SYSTEM and Administrators only. This
//! tool never names that folder, never opens that file, and runs beside the
//! service without touching it. Its test family is a file of its own, in a
//! folder of the user's local application data whose name says what it is:
//! `LiMo-phone-dev-TEST-ONLY`.

use std::io::Write;
use std::path::PathBuf;

use limo_core::HomeCalendar;

/// The folder's name, under the user's local application data.
pub const FOLDER_NAME: &str = "LiMo-phone-dev-TEST-ONLY";
/// The test family's file in it.
pub const FAMILY_FILE: &str = "test-family.limo";
/// The tool's log in it.
pub const LOG_FILE: &str = "phone-dev.log";
/// The log is moved aside past this size: 256 KiB.
pub const LOG_MAX_BYTES: u64 = 256 * 1024;

/// The test family's places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The folder.
    pub dir: PathBuf,
}

impl Places {
    /// The places in this folder (the tests give a temporary one).
    pub fn in_folder(dir: PathBuf) -> Places {
        Places { dir }
    }

    /// The places for this PC's user: `%LOCALAPPDATA%\LiMo-phone-dev-TEST-ONLY`
    /// on Windows; elsewhere, the same name in `$XDG_DATA_HOME` or
    /// `~/.local/share`.
    pub fn for_this_user() -> Result<Places, String> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
            .ok_or_else(|| "This PC's user has no folder for local application data (LOCALAPPDATA is not set).".to_string())?;
        Ok(Places { dir: base.join(FOLDER_NAME) })
    }

    /// The test family's file.
    pub fn family_file(&self) -> PathBuf {
        self.dir.join(FAMILY_FILE)
    }

    /// The journal SQLite may leave beside it.
    pub fn family_journal(&self) -> PathBuf {
        self.dir.join(format!("{FAMILY_FILE}-journal"))
    }

    /// The tool's log.
    pub fn log_file(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }
}

/// A moment in UTC, for the log: "2026-11-20 16:12:03 UTC".
pub fn utc_text(t: i64) -> String {
    let local = HomeCalendar::new(0, 0).local(t);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        local.date.year, local.date.month, local.date.day, local.hour, local.minute, local.second
    )
}

/// Adds one line to the tool's log, with the time. Control characters are
/// taken out, so nothing can forge a line. The log never holds a PIN, a
/// code, a name or a secret: the callers say only what was done.
pub fn append(places: &Places, now: i64, what: &str) -> Result<(), String> {
    std::fs::create_dir_all(&places.dir).map_err(|e| e.to_string())?;
    let path = places.log_file();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX_BYTES) {
        let _ = std::fs::rename(&path, places.dir.join(format!("{LOG_FILE}.old")));
    }
    let clean: String = what.chars().filter(|c| !c.is_control()).collect();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
    writeln!(file, "{}  {clean}", utc_text(now)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_named_for_testing_and_is_not_the_services() {
        let places = Places::in_folder(PathBuf::from("x").join(FOLDER_NAME));
        assert!(places.family_file().ends_with("LiMo-phone-dev-TEST-ONLY/test-family.limo") || places.family_file().ends_with("LiMo-phone-dev-TEST-ONLY\\test-family.limo"));
        assert!(FOLDER_NAME.contains("TEST"));
        assert_ne!(FAMILY_FILE, "family.limo");
    }

    #[test]
    fn a_log_line_has_the_time_in_utc_and_no_control_characters() {
        assert_eq!(utc_text(1_800_000_000), "2027-01-15 08:00:00 UTC");
        let dir = tempfile::tempdir().unwrap();
        let places = Places::in_folder(dir.path().join(FOLDER_NAME));
        append(&places, 1_800_000_000, "setup made\na test family").unwrap();
        append(&places, 1_800_000_060, "forget").unwrap();
        let text = std::fs::read_to_string(places.log_file()).unwrap();
        assert_eq!(text, "2027-01-15 08:00:00 UTC  setup madea test family\n2027-01-15 08:01:00 UTC  forget\n");
    }
}
