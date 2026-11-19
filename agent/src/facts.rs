//! The facts: everything the engine knows of the PC, handed in by the
//! Windows service at every call, as plain data.
//!
//! **What LiMo may know about a program is fixed here.** A program is its
//! file name, the original file name its maker recorded inside the file, and
//! its maker, and nothing else. There is no field for a window title, a
//! document, a file being edited, a web address or anything typed, anywhere
//! in this crate, so the engine cannot record one, pass one to the hub or
//! put one in an order. LiMo records which program was used and for how
//! long, and nothing else.

use serde::{Deserialize, Serialize};

/// What the service knows at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// The master PC's wall clock, unix seconds. It says what time it is:
    /// the day, the blocked hours, when a lock ends. It is never trusted for
    /// how long anything lasted.
    pub wall_secs: i64,
    /// A steady count of milliseconds that cannot be set and only grows
    /// while the PC runs. Every duration the engine counts comes from it.
    /// Best is one that keeps counting while the PC sleeps (on Windows, the
    /// interrupt time, as `QueryInterruptTime` gives it); one that stops
    /// during sleep works too, but then a sleep looks like the wall clock
    /// moving forward and is noted as such.
    pub steady_ms: u64,
    /// The PC's time zone now, as seconds from UTC. Only used to notice a
    /// change, and to tell the guardian on which of the PC's days an account
    /// that is not linked was used. Days and blocked hours follow each
    /// child's home zone, which is in the child's rules.
    pub utc_offset_secs: i32,
    /// Every logged in Windows session.
    pub sessions: Vec<Session>,
}

/// How a Windows session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The session in use: on the screen, and not locked by Windows.
    InUse,
    /// Locked by Windows: its sign in screen is showing.
    Locked,
    /// Logged in but not on the screen, such as after switching user.
    Disconnected,
}

/// One logged in Windows session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The session's id, as Windows gives it.
    pub session_id: u32,
    /// The account's identifier: the same opaque string the hub links to a
    /// child (the service may use the account's security identifier).
    pub account_id: String,
    /// The account's name as Windows shows it, for the guardian's record of
    /// an account that is not linked.
    pub account_name: String,
    /// Whether it is the one in use, locked or disconnected.
    pub state: SessionState,
    /// True when the account is in the Windows Administrators group, even
    /// when it is not running elevated. An account that is not linked and is
    /// an administrator is never locked, whatever the setting for accounts
    /// that are not linked says (the owner's decision of 18 Nov 2026). False
    /// when not given.
    #[serde(default)]
    pub is_administrator: bool,
    /// Seconds since the last keyboard or mouse input in this session.
    pub idle_secs: u32,
    /// True when something plays in full screen in this session: a video or
    /// a game.
    pub full_screen: bool,
    /// The program that owns the window in front, if any.
    pub in_front: Option<Program>,
    /// The programs running in this session.
    pub running: Vec<Program>,
}

/// A program, described only by what identifies the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Program {
    /// Its file name, such as "Minecraft.exe". Only the name: a folder in
    /// front of it is dropped.
    pub file_name: String,
    /// The original file name its maker recorded inside the file (the
    /// version information's `OriginalFilename`), when there is one.
    /// Renaming the file does not change it.
    pub original_name: Option<String>,
    /// Its maker, as recorded inside the file (`CompanyName`), when there is
    /// one.
    pub maker: Option<String>,
}

/// The last part of a path, lower case, with spaces trimmed.
fn plain_name(name: &str) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name);
    base.trim().to_lowercase()
}

impl Program {
    /// The name LiMo records the program under and the rules list it by: the
    /// original file name when the file has one, otherwise the file name, in
    /// lower case. So a renamed program keeps its name, as long as its maker
    /// recorded one.
    pub fn app_id(&self) -> String {
        match self.original_name.as_deref().map(plain_name) {
            Some(original) if !original.is_empty() => original,
            _ => plain_name(&self.file_name),
        }
    }

    /// The file name as it is now, in lower case.
    pub fn file_name_id(&self) -> String {
        plain_name(&self.file_name)
    }

    /// True when the rules' list of programs names this one: by its
    /// [`Program::app_id`], or, when `either_name` is set, by its file name
    /// as it is now. Case is ignored.
    pub(crate) fn listed_in(&self, list: &[String], either_name: bool) -> bool {
        self.listed_as(list, either_name).is_some()
    }

    /// The name by which the rules' list names this program, as
    /// [`Program::listed_in`] finds it, in lower case.
    pub(crate) fn listed_as(&self, list: &[String], either_name: bool) -> Option<String> {
        let id = self.app_id();
        let file = self.file_name_id();
        list.iter().map(|entry| plain_name(entry)).find(|entry| *entry == id || (either_name && *entry == file))
    }

    /// The name to show in plain words: the file name as it is now.
    pub(crate) fn shown_name(&self) -> String {
        let base = self.file_name.rsplit(['\\', '/']).next().unwrap_or(&self.file_name).trim();
        if base.is_empty() { self.app_id() } else { base.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(file: &str, original: Option<&str>) -> Program {
        Program { file_name: file.into(), original_name: original.map(Into::into), maker: None }
    }

    #[test]
    fn a_program_is_known_by_its_original_name_when_it_has_one() {
        let renamed = program("homework.exe", Some("RobloxPlayerBeta.exe"));
        assert_eq!(renamed.app_id(), "robloxplayerbeta.exe");
        assert_eq!(renamed.file_name_id(), "homework.exe");
        assert_eq!(program("Game.EXE", None).app_id(), "game.exe");
        assert_eq!(program("Game.exe", Some("  ")).app_id(), "game.exe");
        // A folder in front of a name is dropped.
        assert_eq!(program(r"C:\Users\Asha\Downloads\game.exe", None).app_id(), "game.exe");
        assert_eq!(program("game.exe", Some(r"D:\build\Original.exe")).app_id(), "original.exe");
        assert_eq!(program(r"C:\x\Game.exe", None).shown_name(), "Game.exe");
    }

    #[test]
    fn a_list_names_a_program_by_its_original_name_and_for_blocking_by_either() {
        let renamed = program("calculator.exe", Some("Game.exe"));
        let list = vec!["Calculator.exe".to_string()];
        assert!(!renamed.listed_in(&list, false));
        assert!(renamed.listed_in(&list, true));
        let blocked = vec!["game.exe".to_string()];
        assert!(renamed.listed_in(&blocked, false));
    }
}
