//! Pure logic: names, paths, clamps and deadline arithmetic.
//!
//! Nothing in here touches the Win32 API, so it can be unit tested on any
//! platform and holds the numbers that the safety rules depend on.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Internal service name, used with the service control manager.
pub const SERVICE_NAME: &str = "LiMoSpike";
/// Friendly name shown in the Services console. It says plainly that the
/// service is a throwaway proof and can be removed.
pub const SERVICE_DISPLAY: &str = "LiMo spike (safe to remove)";

/// The lock can never last longer than this many seconds. It is enforced in
/// two independent places: the `lock` command clamps the deadline it writes,
/// and the managed overlay caps its own lifetime regardless of the file.
pub const HELPER_CAP_SECS: u64 = 120;

/// Folder under Program Files that holds the installed binary.
pub const INSTALL_DIR_NAME: &str = "LiMoSpike";
/// Folder under ProgramData that holds config, control and log files.
pub const DATA_DIR_NAME: &str = "LiMoSpike";

/// Which account the service uses to launch the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperMode {
    /// Launch the helper as the logged in user.
    User,
    /// Launch the helper as SYSTEM inside the user's session.
    System,
    /// Launch the helper as the logged in user, but at medium plus integrity,
    /// one step above the user's ordinary programs. Windows' integrity rules
    /// are meant to keep those programs from ending it, injecting into it or
    /// posting messages to it, while a bug in it yields only the user's own
    /// rights.
    Guarded,
}

impl HelperMode {
    pub fn as_str(self) -> &'static str {
        match self {
            HelperMode::User => "user",
            HelperMode::System => "system",
            HelperMode::Guarded => "guarded",
        }
    }

    /// Parse a mode name. Unknown text falls back to `User`, the safer default.
    pub fn parse(s: &str) -> HelperMode {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => HelperMode::System,
            "guarded" => HelperMode::Guarded,
            _ => HelperMode::User,
        }
    }
}

/// Clamp a requested developer overlay duration to the allowed range 1..=30.
pub fn clamp_overlay_seconds(n: u64) -> u64 {
    n.clamp(1, 30)
}

/// Clamp a requested lock duration to the allowed range 5..=120.
pub fn clamp_lock_seconds(n: u64) -> u64 {
    n.clamp(5, HELPER_CAP_SECS)
}

/// Current unix time in whole seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse the one line of the control file into a unix deadline.
/// Anything that does not parse is treated as 0, meaning "no lock".
pub fn parse_deadline(contents: &str) -> i64 {
    contents.trim().parse::<i64>().unwrap_or(0)
}

/// True when a deadline is further ahead of now than the cap allows. A
/// deadline written by `lock` can never be, because `lock` clamps to the cap,
/// so such a value came from a hand edit or a clock change.
pub fn deadline_too_far(deadline: i64, now: i64) -> bool {
    deadline.saturating_sub(now) > HELPER_CAP_SECS as i64
}

/// The deadline every reader must use: the raw value, or 0 ("no lock") when it
/// is further ahead than the cap. This keeps a lock from ever being in force
/// for more than HELPER_CAP_SECS, whatever the control file says.
pub fn sane_deadline(deadline: i64, now: i64) -> i64 {
    if deadline_too_far(deadline, now) {
        0
    } else {
        deadline
    }
}

/// Seconds remaining until the deadline, never negative.
pub fn remaining_secs(deadline: i64, now: i64) -> i64 {
    // saturating, so a hand edited extreme value cannot overflow and panic
    deadline.saturating_sub(now).max(0)
}

/// Recover the plain Win32 error code from an HRESULT. Win32 errors arrive
/// wrapped as 0x8007xxxx; anything else is returned as the raw HRESULT bits.
pub fn win32_code_from_hresult(hr: i32) -> u32 {
    let bits = hr as u32;
    if bits & 0xFFFF_0000 == 0x8007_0000 {
        bits & 0xFFFF
    } else {
        bits
    }
}

/// Case insensitive path equality, treating forward and back slashes alike.
/// Windows paths are not case sensitive.
pub fn same_path(a: &str, b: &str) -> bool {
    normalize_path(a) == normalize_path(b)
}

/// True when `child` is `parent` itself or lies somewhere inside it.
pub fn path_is_inside(child: &str, parent: &str) -> bool {
    let c = normalize_path(child);
    let p = normalize_path(parent);
    if p.is_empty() {
        return false;
    }
    c == p || c.starts_with(&format!("{p}\\"))
}

fn normalize_path(p: &str) -> String {
    let mut s = p.replace('/', "\\").to_lowercase();
    // canonicalize() adds this prefix; strip it so both sides compare alike.
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// True when the lock is currently in force.
pub fn lock_holds(deadline: i64, now: i64) -> bool {
    now < deadline
}

/// Program Files base folder. Falls back to the common default if the
/// environment variable is missing.
pub fn program_files_dir() -> PathBuf {
    let base = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    base.join(INSTALL_DIR_NAME)
}

/// ProgramData base folder. Falls back to the common default if the
/// environment variable is missing.
pub fn program_data_dir() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join(DATA_DIR_NAME)
}

/// Full path of the installed binary.
pub fn installed_exe() -> PathBuf {
    program_files_dir().join("limo-spike.exe")
}

pub fn config_path() -> PathBuf {
    program_data_dir().join("config.txt")
}

pub fn control_path() -> PathBuf {
    program_data_dir().join("control.txt")
}

pub fn log_path() -> PathBuf {
    program_data_dir().join("spike.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_seconds_are_clamped_to_1_30() {
        assert_eq!(clamp_overlay_seconds(0), 1);
        assert_eq!(clamp_overlay_seconds(1), 1);
        assert_eq!(clamp_overlay_seconds(8), 8);
        assert_eq!(clamp_overlay_seconds(30), 30);
        assert_eq!(clamp_overlay_seconds(31), 30);
        assert_eq!(clamp_overlay_seconds(9999), 30);
    }

    #[test]
    fn lock_seconds_are_clamped_to_5_120() {
        assert_eq!(clamp_lock_seconds(0), 5);
        assert_eq!(clamp_lock_seconds(4), 5);
        assert_eq!(clamp_lock_seconds(5), 5);
        assert_eq!(clamp_lock_seconds(60), 60);
        assert_eq!(clamp_lock_seconds(120), 120);
        assert_eq!(clamp_lock_seconds(121), 120);
        assert_eq!(clamp_lock_seconds(100_000), 120);
    }

    #[test]
    fn lock_can_never_exceed_the_cap() {
        for requested in [5u64, 60, 119, 120, 121, 10_000] {
            assert!(clamp_lock_seconds(requested) <= HELPER_CAP_SECS);
        }
    }

    #[test]
    fn deadline_parsing_is_forgiving() {
        assert_eq!(parse_deadline("0"), 0);
        assert_eq!(parse_deadline("  1700000000\n"), 1_700_000_000);
        assert_eq!(parse_deadline(""), 0);
        assert_eq!(parse_deadline("not a number"), 0);
        assert_eq!(parse_deadline("12.5"), 0);
    }

    #[test]
    fn remaining_never_goes_negative() {
        assert_eq!(remaining_secs(100, 40), 60);
        assert_eq!(remaining_secs(100, 100), 0);
        assert_eq!(remaining_secs(100, 500), 0);
        // extreme values from a hand edited file must not overflow
        assert_eq!(remaining_secs(i64::MIN, 1_000_000), 0);
        assert_eq!(remaining_secs(i64::MAX, -1_000_000), i64::MAX);
    }

    #[test]
    fn lock_holds_only_before_the_deadline() {
        assert!(lock_holds(100, 99));
        assert!(!lock_holds(100, 100));
        assert!(!lock_holds(100, 101));
        assert!(!lock_holds(0, 0));
    }

    #[test]
    fn deadline_within_the_cap_is_kept() {
        let now = 1_000_000;
        assert_eq!(sane_deadline(now + 1, now), now + 1);
        assert_eq!(sane_deadline(now + 60, now), now + 60);
        // exactly at the cap is still a valid lock
        assert_eq!(sane_deadline(now + 120, now), now + 120);
    }

    #[test]
    fn deadline_beyond_the_cap_is_ignored() {
        let now = 1_000_000;
        assert_eq!(sane_deadline(now + 121, now), 0);
        assert_eq!(sane_deadline(now + 3_600, now), 0);
        assert_eq!(sane_deadline(i64::MAX, now), 0);
        assert!(deadline_too_far(now + 121, now));
        assert!(!deadline_too_far(now + 120, now));
    }

    #[test]
    fn past_and_cleared_deadlines_pass_through_unchanged() {
        let now = 1_000_000;
        assert_eq!(sane_deadline(0, now), 0);
        assert_eq!(sane_deadline(now - 5, now), now - 5);
        assert_eq!(sane_deadline(i64::MIN, now), i64::MIN);
        assert!(!lock_holds(sane_deadline(now - 5, now), now));
    }

    #[test]
    fn a_sane_lock_never_has_more_than_the_cap_remaining() {
        let now = 1_000_000;
        for ahead in [0i64, 1, 60, 120, 121, 500, 86_400] {
            let d = sane_deadline(now + ahead, now);
            assert!(remaining_secs(d, now) <= HELPER_CAP_SECS as i64);
        }
    }

    #[test]
    fn a_clock_moved_back_makes_the_lock_fail_open() {
        // Lock set for 60 s, then the clock jumps back an hour.
        let deadline = 1_000_060;
        let now_after_jump = 1_000_000 - 3_600;
        assert_eq!(sane_deadline(deadline, now_after_jump), 0);
    }

    #[test]
    fn hresult_unwraps_to_the_win32_code() {
        assert_eq!(win32_code_from_hresult(0x8007_0005_u32 as i32), 5); // access denied
        assert_eq!(win32_code_from_hresult(0x8007_03F0_u32 as i32), 1008); // no token
        assert_eq!(win32_code_from_hresult(0x8007_0424_u32 as i32), 1060);
        // not a wrapped Win32 error: keep the raw bits
        assert_eq!(win32_code_from_hresult(0x8000_4005_u32 as i32), 0x8000_4005);
        assert_eq!(win32_code_from_hresult(0), 0);
    }

    #[test]
    fn same_path_ignores_case_and_slashes() {
        assert!(same_path(
            r"C:\Program Files\LiMoSpike\limo-spike.exe",
            r"c:\program files\limospike\LIMO-SPIKE.EXE"
        ));
        assert!(same_path(r"C:/Program Files/LiMoSpike", r"C:\Program Files\LiMoSpike\"));
        assert!(same_path(r"\\?\C:\Program Files\X", r"C:\Program Files\X"));
        assert!(!same_path(r"C:\Program Files\LiMoSpike\a.exe", r"C:\Program Files\LiMoSpike\b.exe"));
    }

    #[test]
    fn path_is_inside_respects_folder_boundaries() {
        let dir = r"C:\Program Files\LiMoSpike";
        assert!(path_is_inside(r"C:\Program Files\LiMoSpike\limo-spike.exe", dir));
        assert!(path_is_inside(r"\\?\C:\PROGRAM FILES\limospike\limo-spike.exe", dir));
        assert!(path_is_inside(dir, dir));
        assert!(!path_is_inside(r"C:\Program Files\LiMoSpikeOther\limo-spike.exe", dir));
        assert!(!path_is_inside(r"C:\work\target\release\limo-spike.exe", dir));
        assert!(!path_is_inside(r"C:\anything", ""));
    }

    #[test]
    fn helper_mode_parsing_defaults_to_user() {
        assert_eq!(HelperMode::parse("user"), HelperMode::User);
        assert_eq!(HelperMode::parse("System"), HelperMode::System);
        assert_eq!(HelperMode::parse("SYSTEM"), HelperMode::System);
        assert_eq!(HelperMode::parse("garbage"), HelperMode::User);
        assert_eq!(HelperMode::parse(""), HelperMode::User);
    }

    #[test]
    fn guarded_mode_parses_and_is_named_guarded() {
        assert_eq!(HelperMode::parse("guarded"), HelperMode::Guarded);
        assert_eq!(HelperMode::parse("  Guarded\r\n"), HelperMode::Guarded);
        assert_eq!(HelperMode::parse("GUARDED"), HelperMode::Guarded);
        assert_eq!(HelperMode::Guarded.as_str(), "guarded");
        // a near miss is still unknown text, so it falls back to user
        assert_eq!(HelperMode::parse("guard"), HelperMode::User);
    }

    #[test]
    fn every_mode_name_reads_back_as_the_same_mode() {
        for mode in [HelperMode::User, HelperMode::System, HelperMode::Guarded] {
            assert_eq!(HelperMode::parse(mode.as_str()), mode);
        }
    }
}
