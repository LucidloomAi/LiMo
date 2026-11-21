//! Everything this program asks of the phone, in one place, and what the
//! phone answers.
//!
//! The phone is asked only for what is here: three properties and one
//! setting read, whether the test app is installed, the test app's guarded
//! door written and read, the test app started, and USB debugging switched
//! off. Installing and removing go through each route's own call, and only
//! ever for the test app's package ([`PACKAGE`]). No command here names any
//! other app, an account, a device owner or any other setting; a test in
//! `tests/house_rules.rs` reads every command to check it.

#![forbid(unsafe_code)]

/// The test app's package. The only app this program installs or removes.
pub const PACKAGE: &str = "ai.lucidloom.limo.spike07";
/// The test app's screen.
pub const ACTIVITY: &str = "ai.lucidloom.limo.spike07/.MainActivity";
/// The guarded door's address for the secret: written from standard input.
pub const DOOR_OFFER: &str = "content://ai.lucidloom.limo.spike07.cable/offer";
/// The guarded door's address for what the test app has seen: read only.
pub const DOOR_STATUS: &str = "content://ai.lucidloom.limo.spike07.cable/status";
/// The port on the phone's own loopback that `reverse` carries to this PC,
/// the port the link uses.
pub const LOOPBACK_PORT: u16 = 47810;

/// One thing this program asks of the phone with a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// The phone's model, as Android names it.
    Model,
    /// The Android version, such as 13.
    AndroidVersion,
    /// The Android API level, such as 33.
    Sdk,
    /// Whether the test app is installed.
    IsInstalled,
    /// Whether USB debugging is on, read: 1 or 0.
    AdbEnabled,
    /// The secret into the test app's door, the secret on standard input.
    DoorWrite,
    /// What the test app's door has seen: never the secret itself.
    DoorStatus,
    /// The test app's screen.
    Start,
    /// The test app's screen, asked to run its loopback client to this port.
    StartLoopback(u16),
    /// USB debugging switched off. The last thing asked: the link ends there.
    DebuggingOff,
}

impl Ask {
    /// The shell command, exactly as it crosses the cable.
    pub fn command(&self) -> String {
        match self {
            Ask::Model => "getprop ro.product.model".to_string(),
            Ask::AndroidVersion => "getprop ro.build.version.release".to_string(),
            Ask::Sdk => "getprop ro.build.version.sdk".to_string(),
            Ask::IsInstalled => format!("pm list packages {PACKAGE}"),
            Ask::AdbEnabled => "settings get global adb_enabled".to_string(),
            Ask::DoorWrite => format!("content write --uri {DOOR_OFFER}"),
            Ask::DoorStatus => format!("content query --uri {DOOR_STATUS}"),
            Ask::Start => format!("am start -n {ACTIVITY}"),
            Ask::StartLoopback(port) => format!("am start -n {ACTIVITY} --ei loopback_port {port}"),
            Ask::DebuggingOff => "settings put global adb_enabled 0".to_string(),
        }
    }

    /// Every kind of command, for the house rules' test.
    pub fn every() -> Vec<Ask> {
        vec![
            Ask::Model,
            Ask::AndroidVersion,
            Ask::Sdk,
            Ask::IsInstalled,
            Ask::AdbEnabled,
            Ask::DoorWrite,
            Ask::DoorStatus,
            Ask::Start,
            Ask::StartLoopback(LOOPBACK_PORT),
            Ask::DebuggingOff,
        ]
    }

    /// True for the one command that changes a setting of the phone.
    pub fn changes_a_setting(&self) -> bool {
        matches!(self, Ask::DebuggingOff)
    }
}

/// The text of an answer, each line trimmed (so that a carriage return that
/// Google's adb adds on Windows changes nothing), empty lines left out.
pub fn lines(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// The first line of an answer, trimmed; empty when there is none.
pub fn first_line(text: &str) -> String {
    lines(text).into_iter().next().unwrap_or_default()
}

/// True when `pm list packages` lists the test app itself, not merely a
/// package whose name starts with it.
pub fn lists_the_test_app(text: &str) -> bool {
    lines(text).iter().any(|l| l == &format!("package:{PACKAGE}"))
}

/// True when `am start` says it started the screen: Android's answer starts
/// with "Starting:" and holds no "Error".
pub fn started(text: &str) -> bool {
    let all = lines(text);
    all.iter().any(|l| l.starts_with("Starting:")) && !all.iter().any(|l| l.starts_with("Error"))
}

/// What the test app's door says it has seen, from `content query`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoorStatus {
    /// How many writes the door has taken or refused since the app started.
    pub takes: u32,
    /// `none`, `taken` or `refused`.
    pub secret: String,
    /// How many bytes the last write held.
    pub bytes: u32,
    /// The first 8 hex digits of the SHA-256 of what the last write held.
    pub check: String,
    /// The Linux user id of the program that wrote: 2000 is the adb shell.
    pub caller_uid: i64,
    /// Why the last write was refused, or `none`.
    pub why: String,
    /// How many loopback runs the app has finished since it started.
    pub loopbacks: u32,
    /// `none`, `ok` or `failed`.
    pub loopback: String,
    /// How many bytes came back in the last loopback run.
    pub loopback_bytes: u64,
    /// How long the last loopback run took, in milliseconds.
    pub loopback_ms: u64,
    /// Why the last loopback run failed, or `none`.
    pub loopback_why: String,
}

/// Reads the one row of `content query` on the door's status:
/// `Row: 0 takes=1, secret=taken, bytes=163, ...`. None when the answer holds
/// no such row (the app missing, the door refusing, Android's own error).
pub fn door_status(text: &str) -> Option<DoorStatus> {
    let row = lines(text).into_iter().find(|l| l.starts_with("Row:"))?;
    let rest = row.strip_prefix("Row:")?.trim_start();
    let (_index, fields) = rest.split_once(' ')?;
    let mut status = DoorStatus::default();
    let mut seen = 0;
    for field in fields.split(", ") {
        let (key, value) = field.split_once('=')?;
        let value = value.trim();
        seen += 1;
        match key.trim() {
            "takes" => status.takes = value.parse().ok()?,
            "secret" => status.secret = value.to_string(),
            "bytes" => status.bytes = value.parse().ok()?,
            "check" => status.check = value.to_string(),
            "caller_uid" => status.caller_uid = value.parse().ok()?,
            "why" => status.why = value.to_string(),
            "loopbacks" => status.loopbacks = value.parse().ok()?,
            "loopback" => status.loopback = value.to_string(),
            "loopback_bytes" => status.loopback_bytes = value.parse().ok()?,
            "loopback_ms" => status.loopback_ms = value.parse().ok()?,
            "loopback_why" => status.loopback_why = value.to_string(),
            _ => seen -= 1,
        }
    }
    (seen == 11).then_some(status)
}

/// The last four characters of a serial number, the only part ever shown:
/// `...1A2B`. A serial of four characters or fewer is shown as `...` alone.
pub fn masked_serial(serial: &str) -> String {
    let chars: Vec<char> = serial.trim().chars().collect();
    if chars.len() <= 4 {
        return "...".to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("...{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_as_it_crosses_the_cable() {
        assert_eq!(Ask::IsInstalled.command(), "pm list packages ai.lucidloom.limo.spike07");
        assert_eq!(Ask::DoorWrite.command(), "content write --uri content://ai.lucidloom.limo.spike07.cable/offer");
        assert_eq!(Ask::DoorStatus.command(), "content query --uri content://ai.lucidloom.limo.spike07.cable/status");
        assert_eq!(Ask::Start.command(), "am start -n ai.lucidloom.limo.spike07/.MainActivity");
        assert_eq!(
            Ask::StartLoopback(47810).command(),
            "am start -n ai.lucidloom.limo.spike07/.MainActivity --ei loopback_port 47810"
        );
        assert_eq!(Ask::DebuggingOff.command(), "settings put global adb_enabled 0");
        assert_eq!(Ask::every().iter().filter(|a| a.changes_a_setting()).count(), 1);
    }

    #[test]
    fn answers_are_read_the_same_with_carriage_returns() {
        assert!(lists_the_test_app("package:ai.lucidloom.limo.spike07\r\n"));
        assert!(!lists_the_test_app("package:ai.lucidloom.limo.spike07x\n"));
        assert!(!lists_the_test_app(""));
        assert!(started("Starting: Intent { cmp=ai.lucidloom.limo.spike07/.MainActivity }\r\n"));
        assert!(!started("Starting: Intent { cmp=x }\r\nError type 3\r\nError: Activity class does not exist.\r\n"));
        assert_eq!(first_line("\r\n13\r\n"), "13");
    }

    #[test]
    fn the_door_status_row_is_read_whole_or_not_at_all() {
        let row = "Row: 0 takes=2, secret=taken, bytes=163, check=1a2b3c4d, caller_uid=2000, why=none, \
                   loopbacks=1, loopback=ok, loopback_bytes=262174, loopback_ms=85, loopback_why=none\r\n";
        let status = door_status(row).unwrap();
        assert_eq!(status.takes, 2);
        assert_eq!(status.secret, "taken");
        assert_eq!(status.bytes, 163);
        assert_eq!(status.check, "1a2b3c4d");
        assert_eq!(status.caller_uid, 2000);
        assert_eq!(status.loopback, "ok");
        assert_eq!(status.loopback_bytes, 262_174);
        assert_eq!(door_status("No result found.\n"), None);
        assert_eq!(door_status("Row: 0 takes=2, secret=taken\n"), None);
        assert_eq!(door_status("Error while accessing provider:ai.lucidloom.limo.spike07.cable\n"), None);
    }

    #[test]
    fn a_serial_is_shown_by_its_last_four_characters_only() {
        assert_eq!(masked_serial("R5CN20ABCD1A2B"), "...1A2B");
        assert_eq!(masked_serial("ABCD"), "...");
        assert_eq!(masked_serial(""), "...");
    }
}
