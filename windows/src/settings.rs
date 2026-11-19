//! Names, places and the data folder's settings file.
//!
//! When installed, LiMo for Windows is one program and one folder of data:
//!
//! - `%ProgramFiles%\LiMo\limo.exe`, the program;
//! - `%ProgramData%\LiMo\`, for SYSTEM and Administrators only, holding
//!   `family.limo` (the family's file), `limo.log` (the service's log, and
//!   `limo.log.old` before it) and `settings.json` (the test cap, and the
//!   switch that forces the plain display).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The service's name for the service control manager.
pub const SERVICE_NAME: &str = "LiMo";
/// The service's name as the Services console shows it.
pub const SERVICE_DISPLAY: &str = "LiMo screen time";
/// The service's description in the Services console.
pub const SERVICE_DESCRIPTION: &str =
    "LiMo, a free screen time limiter for children: counts and limits screen time on this PC.";

/// The folder under Program Files, and the one under ProgramData.
pub const FOLDER_NAME: &str = "LiMo";
/// The program's file name.
pub const EXE_NAME: &str = "limo.exe";
/// The family's file, in the data folder.
pub const FAMILY_FILE: &str = "family.limo";
/// The service's log, in the data folder.
pub const LOG_FILE: &str = "limo.log";
/// The log before the newest, kept beside it.
pub const OLD_LOG_FILE: &str = "limo.log.old";
/// The settings file, in the data folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// When the log grows past this many bytes it is moved to
/// [`OLD_LOG_FILE`] and begun again: 1 MiB.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// With the test cap on, a lock that has been on for this many seconds is
/// lifted: 120.
pub const TEST_CAP_SECS: u64 = 120;

/// The data folder's protection: SYSTEM and Administrators get full
/// control, and nobody else gets anything; nothing is inherited from
/// ProgramData. Folders and files made inside it get the same.
pub const DATA_FOLDER_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

/// What the settings file holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// True to lift any single lock after [`TEST_CAP_SECS`]: for testing
    /// with the owner present only. Written by `limo install --dev`.
    #[serde(default)]
    pub test_cap: bool,
    /// True to have each helper started from now on show the plain display
    /// instead of the drawn lock screen and time left display: for testing
    /// only. Written by hand.
    #[serde(default)]
    pub plain_display: bool,
}

/// The settings in a settings file's text. Anything that cannot be read is
/// no setting at all, so a damaged file never switches the test cap on.
pub fn read_settings(text: &str) -> Settings {
    serde_json::from_str(text).unwrap_or_default()
}

/// The text of a settings file.
pub fn settings_text(settings: &Settings) -> String {
    let mut text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

/// Where LiMo's folders are, from the two Windows folders they live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The program's folder, `%ProgramFiles%\LiMo`.
    pub install_dir: PathBuf,
    /// The data folder, `%ProgramData%\LiMo`.
    pub data_dir: PathBuf,
}

impl Places {
    /// LiMo's places under the given Program Files and ProgramData folders.
    pub fn under(program_files: &Path, program_data: &Path) -> Places {
        Places { install_dir: program_files.join(FOLDER_NAME), data_dir: program_data.join(FOLDER_NAME) }
    }

    /// LiMo's places on this PC, from the environment, or Windows' usual
    /// folders when it does not say.
    pub fn here() -> Places {
        let program_files = std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
        let program_data = std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
        Places::under(&program_files, &program_data)
    }

    /// The installed program.
    pub fn installed_exe(&self) -> PathBuf {
        self.install_dir.join(EXE_NAME)
    }

    /// The family's file.
    pub fn family_file(&self) -> PathBuf {
        self.data_dir.join(FAMILY_FILE)
    }

    /// The log.
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join(LOG_FILE)
    }

    /// The log before the newest.
    pub fn old_log_file(&self) -> PathBuf {
        self.data_dir.join(OLD_LOG_FILE)
    }

    /// The settings file.
    pub fn settings_file(&self) -> PathBuf {
        self.data_dir.join(SETTINGS_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_cap_is_on_only_when_the_file_says_so_plainly() {
        let cap = Settings { test_cap: true, plain_display: false };
        assert_eq!(read_settings(&settings_text(&cap)), cap);
        assert_eq!(read_settings(&settings_text(&Settings::default())), Settings::default());
        assert_eq!(read_settings(""), Settings::default());
        assert_eq!(read_settings("{}"), Settings::default());
        assert_eq!(read_settings("test_cap = true"), Settings::default());
        assert_eq!(read_settings(r#"{"test_cap": "yes"}"#), Settings::default());
        assert!(!Settings::default().test_cap);
    }

    #[test]
    fn the_plain_display_is_forced_only_when_the_file_says_so_plainly() {
        assert!(!Settings::default().plain_display);
        // The file the test install writes, with the switch added by hand.
        assert_eq!(read_settings(r#"{"test_cap": true, "plain_display": true}"#), Settings { test_cap: true, plain_display: true });
        assert_eq!(read_settings(r#"{"plain_display": true}"#), Settings { test_cap: false, plain_display: true });
        assert_eq!(read_settings(r#"{"plain_display": 1}"#), Settings::default());
        let both = Settings { test_cap: false, plain_display: true };
        assert_eq!(read_settings(&settings_text(&both)), both);
    }

    #[test]
    fn the_places_are_under_the_two_windows_folders() {
        let places = Places::under(Path::new(r"C:\Program Files"), Path::new(r"C:\ProgramData"));
        assert_eq!(places.installed_exe(), PathBuf::from(r"C:\Program Files\LiMo\limo.exe"));
        assert_eq!(places.family_file(), PathBuf::from(r"C:\ProgramData\LiMo\family.limo"));
        assert_eq!(places.log_file(), PathBuf::from(r"C:\ProgramData\LiMo\limo.log"));
        assert_eq!(places.settings_file(), PathBuf::from(r"C:\ProgramData\LiMo\settings.json"));
        // Users, Authenticated Users and Everyone are not in the folder's
        // protection at all, and nothing is inherited.
        for nobody in ["BU", "AU", "WD", "IU", "S-1-5-32-545"] {
            assert!(!DATA_FOLDER_SDDL.contains(&format!(";;;{nobody})")), "{nobody}");
        }
        assert!(DATA_FOLDER_SDDL.starts_with("D:P"));
    }
}
