//! The window's own log: what the door said, and why it could not be asked,
//! for a person to read when the window says LiMo's service could not be
//! reached. It is the account's own file, `%LOCALAPPDATA%\LiMo\console.log`
//! (beside LiMo's key for phones), which the signed in account can read and
//! no other standard account can.
//!
//! It holds the door's notes (who served the pipe, what was sent, why a
//! connection was dropped) and the requests that could not be asked, by
//! their names. Never a request's arguments, never a password or a PIN.
//! Before a line is written, anything of an account is taken out: a
//! security identifier that is not a well known one (LocalSystem, the
//! Administrators group and the like stay, a person's own does not), a
//! folder under `\Users\`, and this account's and this PC's names.
//!
//! It is kept short: once it passes about 96 KB, only its last 300 lines are
//! kept. Writing it never stops the window: a line that cannot be written
//! is let go.

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The file's name.
pub const LOG_FILE: &str = "console.log";
/// Where it is, as a person types it into Explorer's address bar.
pub const SHOWN_PLACE: &str = r"%LOCALAPPDATA%\LiMo\console.log";
/// The lines kept when it is cut back.
pub const KEEP_LINES: usize = 300;
/// The size past which it is cut back.
pub const ROLL_BYTES: u64 = 96 * 1024;

/// The log.
#[derive(Debug)]
pub struct ConsoleLog {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

/// Security identifiers that name no person: they stay as they are.
fn well_known(sid: &str) -> bool {
    matches!(sid, "S-1-1-0" | "S-1-5-4" | "S-1-5-11" | "S-1-5-18" | "S-1-5-19" | "S-1-5-20") || sid.starts_with("S-1-5-32-")
}

/// `text` with anything of an account taken out (see the module's words).
pub fn without_accounts(text: &str) -> String {
    // Security identifiers.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("S-1-") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let digits = tail[4..].find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(tail.len() - 4);
        let sid = tail[..4 + digits].trim_end_matches('-');
        if sid.len() <= 3 {
            out.push_str("S-1-");
            rest = &tail[4..];
            continue;
        }
        if well_known(sid) {
            out.push_str(sid);
        } else {
            out.push_str("(an account's own identifier)");
        }
        rest = &tail[sid.len()..];
    }
    out.push_str(rest);
    // A folder under \Users\ (or /Users/) names an account.
    let mut text = out;
    for sep in ['\\', '/'] {
        let marker = format!("{sep}users{sep}");
        let mut done = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = find_ignoring_ascii_case(rest, &marker) {
            let start = at + marker.len();
            done.push_str(&rest[..start]);
            let after = &rest[start..];
            let end = after.find(|c: char| c == '\\' || c == '/' || c == '"' || c.is_whitespace()).unwrap_or(after.len());
            done.push_str("(an account)");
            rest = &after[end..];
        }
        done.push_str(rest);
        text = done;
    }
    // This account's and this PC's own names, wherever else they are.
    for (var, instead) in [("USERNAME", "(this account)"), ("COMPUTERNAME", "(this PC)")] {
        if let Ok(name) = std::env::var(var)
            && name.chars().count() >= 3
        {
            text = replace_ignoring_case(&text, &name, instead);
        }
    }
    text
}

/// Where `needle` first is in `hay`, ignoring the case of ASCII letters: a
/// byte offset into `hay` itself.
fn find_ignoring_ascii_case(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| hay.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn replace_ignoring_case(text: &str, what: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = find_ignoring_ascii_case(rest, what) {
        out.push_str(&rest[..at]);
        out.push_str(with);
        rest = &rest[at + what.len()..];
    }
    out.push_str(rest);
    out
}

/// The time now, as `2026-11-21 20:12:52 UTC`.
fn now_text() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, of_day) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", of_day / 3_600, of_day % 3_600 / 60, of_day % 60)
}

impl ConsoleLog {
    /// The log of the account the window runs in; none when the account
    /// has no local application data.
    pub fn for_this_account() -> ConsoleLog {
        let path = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|v| PathBuf::from(v).join("LiMo").join(LOG_FILE));
        ConsoleLog { path, lock: Mutex::new(()) }
    }

    /// A log at `path`: the self check's, and the tests'.
    pub fn at(path: &Path) -> ConsoleLog {
        ConsoleLog { path: Some(path.to_path_buf()), lock: Mutex::new(()) }
    }

    /// Its file.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Writes one line, with the time, and anything of an account taken
    /// out; cuts the file back when it has grown past [`ROLL_BYTES`].
    pub fn write(&self, what: &str) {
        let Some(path) = &self.path else { return };
        let _held = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let line = format!("{}  {}\n", now_text(), without_accounts(what).replace(['\r', '\n'], " "));
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let written = std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| f.write_all(line.as_bytes()));
        if written.is_err() {
            return;
        }
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > ROLL_BYTES
            && let Ok(text) = std::fs::read_to_string(path)
        {
            let lines: Vec<&str> = text.lines().collect();
            let kept = lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n";
            let fresh = path.with_extension("log.new");
            if std::fs::write(&fresh, kept).is_ok() {
                let _ = std::fs::rename(&fresh, path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_of_an_account_is_written_and_well_known_ones_stay() {
        let said = without_accounts(
            r"the pipe is served by process 4 in session 0, owned by S-1-5-32-544; another by S-1-5-21-1111-2222-3333-1001, from C:\Users\Mira\Downloads\limo.exe and C:/Users/Asha/x",
        );
        assert!(said.contains("S-1-5-32-544"), "{said}");
        assert!(!said.contains("S-1-5-21") && !said.contains("1001"), "{said}");
        assert!(said.contains("(an account's own identifier)"), "{said}");
        assert!(said.contains(r"C:\Users\(an account)\Downloads\limo.exe"), "{said}");
        assert!(said.contains("C:/Users/(an account)/x"), "{said}");
        assert!(!said.contains("Mira") && !said.contains("Asha"), "{said}");
        assert_eq!(without_accounts("owned by S-1-5-18."), "owned by S-1-5-18.");
        if let Ok(me) = std::env::var("USERNAME")
            && me.len() >= 3
        {
            assert!(!without_accounts(&format!("a note naming {me} here")).to_lowercase().contains(&me.to_lowercase()));
        }
    }

    #[test]
    fn the_log_keeps_its_last_lines_and_never_grows_past_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = ConsoleLog::at(&dir.path().join("LiMo").join(LOG_FILE));
        for n in 0..2_000 {
            log.write(&format!("note {n}: the door was lost: reading failed with Win32 error 109, and a little more to fill the line"));
        }
        let text = std::fs::read_to_string(log.path().unwrap()).unwrap();
        assert!(text.len() as u64 <= ROLL_BYTES + 200, "{}", text.len());
        assert!(text.lines().count() <= KEEP_LINES + 1_000);
        assert!(text.lines().last().unwrap().contains("note 1999:"));
        assert!(text.lines().next().unwrap().ends_with("and a little more to fill the line"));
        let first = text.lines().next().unwrap();
        assert!(first.len() > 24 && first[..4].parse::<u32>().is_ok() && first[19..24] == *" UTC ", "{first}");
    }

    #[test]
    fn what_a_refused_server_was_is_logged_with_nothing_of_an_account() {
        use crate::door::{ServerMustBe, ServerSeen, seen_words};
        let seen = ServerSeen {
            pid: 4321,
            session: Some(2),
            owner: Some("S-1-5-21-1111-2222-3333-1001".into()),
            image: Some(r"C:\Users\Mira\Downloads\limo.exe".into()),
        };
        let line = without_accounts(&seen_words(&seen, &ServerMustBe::limo_service(r"C:\Program Files")));
        assert_eq!(
            line,
            r"what served the pipe: process 4321 in session 2, the pipe owned by (an account's own identifier), C:\Users\(an account)\Downloads\limo.exe; LiMo's service runs in session 0, its pipe owned by S-1-5-18 or S-1-5-32-544, from C:\Program Files\LiMo\limo.exe"
        );
        let unreadable = ServerSeen { pid: 7, session: None, owner: None, image: None };
        assert!(seen_words(&unreadable, &ServerMustBe::limo_service(r"C:\Program Files")).contains("a program file this account may not read"));
    }

    #[test]
    fn a_log_with_no_place_writes_nothing_and_never_fails() {
        let log = ConsoleLog { path: None, lock: Mutex::new(()) };
        log.write("the door was lost");
        assert!(log.path().is_none());
    }
}
