//! What the tool asks of a phone, and nothing else: picking the one phone
//! attached, reading what it says of itself, and LiMo's own app and files
//! through `run-as`. Every call is an [`Adb`] call; the shell commands are
//! here, in one place, so that the README can say exactly what crosses the
//! cable.
//!
//! **What a call never carries.** No secret is ever put in an argument: the
//! pairing file, the one thing that holds the test family's secrets, goes
//! through adb's standard input ([`Phone::write_pairing`]). The broadcasts
//! carry an action's name and LiMo's receiver, nothing more.

use std::fmt;
use std::time::Duration;

use crate::adb::{Adb, Call, Reply, T_INSTALL, T_SHORT, T_UNINSTALL};

/// The child form's package.
pub const PACKAGE: &str = "ai.lucidloom.limo.child";
/// The guardian form's package: refused by `install`.
pub const GUARDIAN_PACKAGE: &str = "ai.lucidloom.limo";
/// The debug only receiver that takes the tool's broadcasts.
pub const RECEIVER: &str = "ai.lucidloom.limo.child/.dev.DevReceiver";
/// Take the pairing file: make the engine from it, keep it, delete the file.
pub const PAIR_ACTION: &str = "ai.lucidloom.limo.child.dev.PAIR";
/// Wipe LiMo's kept state.
pub const FORGET_ACTION: &str = "ai.lucidloom.limo.child.dev.FORGET";
/// The pairing file, in the app's private files.
pub const PAIRING_FILE: &str = "files/limo-dev/pairing.json";
/// The engine's kept state, in the app's private files.
pub const STATE_FILE: &str = "files/limo/state";
/// The tags of LiMo's log lines: the host's and the watch's.
pub const LOG_TAGS: [&str; 2] = ["LiMo", "LiMoWatch"];
/// The oldest Android LiMo runs on: 8.0, API 26.
pub const OLDEST_SDK: u32 = 26;

/// How long `setup` and `forget` wait for LiMo to answer a broadcast.
pub const ANSWER_WAIT: Duration = Duration::from_secs(15);

// The shell commands, as the phone's shell reads them. `run-as` runs a
// command as LiMo's own app, in its own data folder, and only for a debug
// build: Android refuses it for a release build.

/// What the phone says of itself.
pub fn props_command() -> String {
    "getprop ro.build.version.release; getprop ro.build.version.sdk; getprop ro.product.manufacturer; getprop ro.product.model"
        .to_string()
}

/// Whether the child form is installed.
pub fn listed_command() -> String {
    format!("pm list packages {PACKAGE}")
}

/// What Android keeps of the installed child form: its version and flags.
pub fn dump_command() -> String {
    format!("dumpsys package {PACKAGE}")
}

/// Whether `run-as` works for LiMo: it does only for a debug build.
pub fn probe_command() -> String {
    format!("run-as {PACKAGE} id -u")
}

/// Whether LiMo keeps a state, and whether a pairing file waits.
pub fn look_command() -> String {
    format!("run-as {PACKAGE} sh -c 'test -e {STATE_FILE} && echo state; test -e {PAIRING_FILE} && echo pairing; true'")
}

/// The first line of the answer to [`read_state_command`] when LiMo keeps a
/// state: the state's own bytes follow it.
pub const STATE_FOLLOWS: &[u8] = b"limo-dev-state\n";
/// The whole answer to [`read_state_command`] when LiMo keeps no state.
pub const NO_STATE: &[u8] = b"limo-dev-none\n";

/// LiMo's kept state, sent through `adb exec-out` ([`Phone::read_state`]):
/// a first line saying whether one follows ([`STATE_FOLLOWS`]), then its
/// bytes; or [`NO_STATE`]. The answer says which itself, because the exit
/// code of `exec-out` is always 0.
pub fn read_state_command() -> String {
    format!(
        "run-as {PACKAGE} sh -c 'if test -e {STATE_FILE}; then echo limo-dev-state; cat {STATE_FILE}; else echo limo-dev-none; fi'"
    )
}

/// What [`read_state_command`] answered: the state's exact bytes, None when
/// LiMo keeps none, or what went wrong (`run-as` refusing, say, whose words
/// come on the same stream through `exec-out`).
pub fn parse_state_answer(answer: &[u8]) -> Result<Option<Vec<u8>>, Trouble> {
    if let Some(state) = answer.strip_prefix(STATE_FOLLOWS) {
        return Ok(Some(state.to_vec()));
    }
    if answer == NO_STATE {
        return Ok(None);
    }
    if answer.starts_with(b"limo-dev-state\r\n") || answer == b"limo-dev-none\r\n" {
        return Err(Trouble::Unexpected(
            "LiMo's state came back with its line ends changed (each line feed made a carriage return and a line \
             feed), so it cannot be read as kept."
                .to_string(),
        ));
    }
    Err(Trouble::Unexpected(first_line(&String::from_utf8_lossy(answer))))
}

/// The pairing file from standard input into LiMo's own files, readable by
/// LiMo alone; written under another name first and then moved, so LiMo
/// never sees half of it; then its size, to check.
pub fn write_pairing_command() -> String {
    format!(
        "run-as {PACKAGE} sh -c 'umask 077 && mkdir -p files/limo-dev && cat > {PAIRING_FILE}.part && mv -f {PAIRING_FILE}.part {PAIRING_FILE} && wc -c < {PAIRING_FILE}'"
    )
}

/// A pairing file LiMo did not take, removed.
pub fn remove_pairing_command() -> String {
    format!("run-as {PACKAGE} rm -f {PAIRING_FILE} {PAIRING_FILE}.part")
}

/// A broadcast to LiMo's debug receiver, by its name, so no other app can
/// receive it. Only adb's shell and the system may send to that receiver.
pub fn broadcast_command(action: &str) -> String {
    format!("am broadcast -a {action} -n {RECEIVER} --include-stopped-packages --receiver-foreground")
}

/// The phone's clock: unix seconds, then its zone as `+0530`.
pub fn clock_command() -> String {
    "date +%s; date +%z".to_string()
}

/// One phone or emulator as `adb devices` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLine {
    /// Its serial number.
    pub serial: String,
    /// `device` when ready; `unauthorized`, `offline` and others when not.
    pub state: String,
}

/// `adb devices`: one entry per device line, whatever its state.
pub fn parse_devices(text: &str) -> Vec<DeviceLine> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('*') || t.starts_with("List of devices") || t.starts_with("adb server") || t.starts_with("adb: ") {
            continue;
        }
        let mut parts = t.split_whitespace();
        let (Some(serial), Some(state)) = (parts.next(), parts.next()) else { continue };
        let state = if state == "no" { "no permissions".to_string() } else { state.to_string() };
        out.push(DeviceLine { serial: serial.to_string(), state });
    }
    out
}

/// Why no phone could be picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickProblem {
    /// Nothing is attached.
    NoPhone,
    /// More than one phone or emulator.
    Several(Vec<DeviceLine>),
    /// The phone has not allowed this PC yet.
    NotAllowed(String),
    /// The phone is offline.
    Offline(String),
    /// This PC's account may not use the phone over USB.
    NoPermissions(String),
    /// Any other state, such as recovery or sideload.
    Other(String, String),
    /// `adb devices` did not answer, or could not run.
    NoAnswer(String),
}

impl fmt::Display for PickProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PickProblem::NoPhone => f.write_str(
                "No phone is attached. Plug the phone in with a USB cable, unlock it, and check that USB debugging is on \
                 (Settings, Developer options). This tool never turns it on by itself.",
            ),
            PickProblem::Several(list) => write!(
                f,
                "More than one phone or emulator is attached ({}). Unplug the others or close the emulator, so that this \
                 tool cannot pick the wrong one.",
                list.iter().map(|d| format!("{} {}", d.serial, d.state)).collect::<Vec<_>>().join(", ")
            ),
            PickProblem::NotAllowed(serial) => write!(
                f,
                "The phone {serial} has not allowed this PC yet. Unlock it and tap Allow on the \"Allow USB debugging?\" \
                 prompt. If no prompt shows, unplug the phone and plug it in again."
            ),
            PickProblem::Offline(serial) => {
                write!(f, "The phone {serial} is offline. Unplug it and plug it in again, and unlock it.")
            }
            PickProblem::NoPermissions(serial) => write!(
                f,
                "This PC's account may not use the phone {serial} over USB. On Linux, the phone needs a udev rule; on \
                 Windows, the phone's USB driver."
            ),
            PickProblem::Other(serial, state) => {
                write!(f, "The phone {serial} is in the state \"{state}\", not ready for adb. Start it normally and unlock it.")
            }
            PickProblem::NoAnswer(why) => {
                write!(f, "adb could not list the phones: {why} Run adb kill-server, then this again.")
            }
        }
    }
}

/// The serial of the one phone ready for adb.
pub fn pick(list: &[DeviceLine]) -> Result<String, PickProblem> {
    match list {
        [] => Err(PickProblem::NoPhone),
        [one] => match one.state.as_str() {
            "device" => Ok(one.serial.clone()),
            "unauthorized" => Err(PickProblem::NotAllowed(one.serial.clone())),
            "offline" => Err(PickProblem::Offline(one.serial.clone())),
            "no permissions" => Err(PickProblem::NoPermissions(one.serial.clone())),
            other => Err(PickProblem::Other(one.serial.clone(), other.to_string())),
        },
        many => Err(PickProblem::Several(many.to_vec())),
    }
}

/// Something went wrong talking to a phone that was picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trouble {
    /// adb could not be run.
    Adb(String),
    /// A call did not answer in time.
    TimedOut(u64),
    /// The phone went away: unplugged, or asleep with the cable loose.
    Gone,
    /// The phone stopped allowing this PC.
    NotAllowed,
    /// The phone answered something this tool cannot read.
    Unexpected(String),
}

impl fmt::Display for Trouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trouble::Adb(why) => f.write_str(why),
            Trouble::TimedOut(secs) => write!(
                f,
                "The phone did not answer within {secs} seconds, so the call was stopped. Unplug the phone, plug it in \
                 again and unlock it; if it goes on, run adb kill-server."
            ),
            Trouble::Gone => f.write_str("The phone is no longer there: it was unplugged, or the cable is loose. Plug it in again and unlock it."),
            Trouble::NotAllowed => f.write_str(
                "The phone no longer allows this PC. Unlock it and tap Allow on the \"Allow USB debugging?\" prompt.",
            ),
            Trouble::Unexpected(what) => write!(f, "The phone answered something this tool cannot read: {what}"),
        }
    }
}

/// The first line of some text that is not empty, cut short.
pub fn first_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(nothing)");
    let mut cut: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        cut.push_str("...");
    }
    cut
}

/// adb's own words for a phone that went away or stopped allowing this PC.
fn trouble_in(reply: &Reply) -> Option<Trouble> {
    let text = reply.stderr.to_ascii_lowercase();
    if text.contains("unauthorized") {
        Some(Trouble::NotAllowed)
    } else if text.contains("error: device") && text.contains("not found")
        || text.contains("no devices/emulators found")
        || text.contains("device offline")
        || text.trim() == "error: closed"
    {
        Some(Trouble::Gone)
    } else {
        None
    }
}

/// What the phone says of itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Props {
    /// "13".
    pub release: String,
    /// 33.
    pub sdk: Option<u32>,
    /// "samsung".
    pub maker: String,
    /// "SM-G988B".
    pub model: String,
}

/// What `run-as` said for LiMo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAs {
    /// It works: a debug build.
    Works,
    /// Android refuses it: a release build.
    Release,
    /// Android does not know the package.
    NotInstalled,
    /// It failed for another reason, in Android's words.
    Broken(String),
}

/// What LiMo keeps in its files, as far as the tool looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    /// The engine's kept state is there.
    pub state: bool,
    /// A pairing file waits, not yet taken.
    pub pairing: bool,
}

/// What `adb install` gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Installed.
    Done,
    /// Refused, with Android's code ("INSTALL_FAILED_...") and its first line.
    Refused {
        /// The code, or "unknown".
        code: String,
        /// The first line of what adb said.
        said: String,
    },
}

/// What `adb uninstall` gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Removed.
    Done,
    /// LiMo is a device admin (its protection, or strict mode): Android does
    /// not remove an admin app.
    StillAdmin,
    /// A Device Owner blocks it.
    Blocked,
    /// Android does not know the package.
    NotInstalled,
    /// Refused for another reason, in Android's words.
    Refused(String),
}

/// The phone that was picked, and every call the tool makes to it.
pub struct Phone<'a> {
    adb: &'a mut dyn Adb,
    /// Its serial number.
    pub serial: String,
}

impl<'a> Phone<'a> {
    /// Lists what is attached and picks the one phone that is ready.
    pub fn pick(adb: &'a mut dyn Adb) -> Result<Phone<'a>, PickProblem> {
        let reply =
            adb.run(Call::new(vec!["devices".to_string()], T_SHORT)).map_err(PickProblem::NoAnswer)?;
        if reply.timed_out {
            return Err(PickProblem::NoAnswer("adb devices did not answer within 30 seconds.".to_string()));
        }
        let serial = pick(&parse_devices(&reply.text()))?;
        Ok(Phone { adb, serial })
    }

    /// Runs `adb -s SERIAL ARGS...`.
    fn run(&mut self, args: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> Result<Reply, Trouble> {
        let mut all = vec!["-s".to_string(), self.serial.clone()];
        all.extend(args.iter().map(|a| a.to_string()));
        let reply = self.adb.run(Call { args: all, stdin, timeout, on_line: None }).map_err(Trouble::Adb)?;
        if reply.timed_out {
            return Err(Trouble::TimedOut(timeout.as_secs()));
        }
        if let Some(trouble) = trouble_in(&reply) {
            return Err(trouble);
        }
        Ok(reply)
    }

    /// Runs one shell command on the phone, with no terminal. Its answers
    /// are text, read line by line with each line trimmed, so a line feed
    /// that comes back as a carriage return and a line feed (as `adb shell`
    /// gives it on Windows) changes nothing. Bytes that must arrive exactly
    /// come through `exec-out` instead ([`Phone::read_state`]).
    fn shell(&mut self, command: &str, stdin: Option<&[u8]>) -> Result<Reply, Trouble> {
        self.run(&["shell", "-T", command], stdin, T_SHORT)
    }

    /// Waits.
    pub fn pause(&mut self, d: Duration) {
        self.adb.pause(d);
    }

    /// What the phone says of itself.
    pub fn props(&mut self) -> Result<Props, Trouble> {
        let reply = self.shell(&props_command(), None)?;
        let out = reply.out();
        let mut lines = out.lines().map(str::trim);
        let release = lines.next().unwrap_or_default().to_string();
        let sdk = lines.next().and_then(|s| s.parse().ok());
        let maker = lines.next().unwrap_or_default().to_string();
        let model = lines.next().unwrap_or_default().to_string();
        if release.is_empty() && sdk.is_none() {
            return Err(Trouble::Unexpected(first_line(&reply.text())));
        }
        Ok(Props { release, sdk, maker, model })
    }

    /// Whether the child form is installed.
    pub fn installed(&mut self) -> Result<bool, Trouble> {
        let reply = self.shell(&listed_command(), None)?;
        let text = reply.text();
        if text.contains("Exception") || text.contains("Error:") {
            return Err(Trouble::Unexpected(first_line(&text)));
        }
        let want = format!("package:{PACKAGE}");
        Ok(reply.out().lines().any(|l| l.trim() == want))
    }

    /// The installed child form's version, and whether Android marks it as
    /// a debug build, as far as `dumpsys` says.
    pub fn version_and_flag(&mut self) -> Result<(Option<String>, Option<bool>), Trouble> {
        let reply = self.shell(&dump_command(), None)?;
        Ok(parse_dump(&reply.out()))
    }

    /// Whether `run-as` works for LiMo.
    pub fn probe(&mut self) -> Result<RunAs, Trouble> {
        let reply = self.shell(&probe_command(), None)?;
        Ok(run_as_answer(&reply))
    }

    /// Whether LiMo keeps a state, and whether a pairing file waits.
    pub fn look(&mut self) -> Result<Look, Trouble> {
        let reply = self.shell(&look_command(), None)?;
        if !reply.ok() {
            return Err(Trouble::Unexpected(first_line(&reply.text())));
        }
        let out = reply.out();
        Ok(Look { state: out.lines().any(|l| l.trim() == "state"), pairing: out.lines().any(|l| l.trim() == "pairing") })
    }

    /// LiMo's kept state, exactly as kept; None when it keeps none.
    ///
    /// Read through `adb exec-out`, which hands bytes over as they are.
    /// `adb shell` does not on Windows: the first run on a real phone, on
    /// 20 November 2026, saw every line feed of the state come back as a
    /// carriage return and a line feed, and the engine then took the state
    /// for damaged. The exit code of `exec-out` is always 0, so whether a
    /// state follows is said by the answer's first line instead.
    pub fn read_state(&mut self) -> Result<Option<Vec<u8>>, Trouble> {
        let reply = self.run(&["exec-out", &read_state_command()], None, T_SHORT)?;
        parse_state_answer(&reply.stdout)
    }

    /// Writes the pairing file into LiMo's own files through standard
    /// input, and checks that all of it arrived.
    pub fn write_pairing(&mut self, bytes: &[u8]) -> Result<(), Trouble> {
        let reply = self.shell(&write_pairing_command(), Some(bytes))?;
        if !reply.ok() {
            return Err(Trouble::Unexpected(first_line(&reply.text())));
        }
        match reply.out().trim().parse::<usize>() {
            Ok(n) if n == bytes.len() => Ok(()),
            Ok(n) => Err(Trouble::Unexpected(format!("the pairing file arrived with {n} bytes of {}.", bytes.len()))),
            Err(_) => Err(Trouble::Unexpected(first_line(&reply.text()))),
        }
    }

    /// Removes a pairing file LiMo did not take.
    pub fn remove_pairing(&mut self) -> Result<(), Trouble> {
        self.shell(&remove_pairing_command(), None).map(|_| ())
    }

    /// Sends a broadcast to LiMo's debug receiver; Android's answer line.
    pub fn broadcast(&mut self, action: &str) -> Result<String, Trouble> {
        let reply = self.shell(&broadcast_command(action), None)?;
        let text = reply.text();
        if text.contains("Exception") || text.contains("Error:") || !reply.ok() {
            return Err(Trouble::Unexpected(first_line(&text)));
        }
        Ok(text.lines().map(str::trim).find(|l| l.starts_with("Broadcast completed")).unwrap_or("Broadcast sent.").to_string())
    }

    /// The phone's clock and zone: unix seconds and seconds from UTC.
    pub fn clock(&mut self) -> Result<Option<(i64, i32)>, Trouble> {
        let reply = self.shell(&clock_command(), None)?;
        Ok(parse_clock(&reply.out()))
    }

    /// Installs an APK, replacing LiMo and keeping its files if it is there.
    pub fn install(&mut self, path: &str) -> Result<Installed, Trouble> {
        let reply = self.run(&["install", "-r", "-t", path], None, T_INSTALL)?;
        Ok(parse_install(&reply.text()))
    }

    /// Removes LiMo, with everything it kept.
    pub fn uninstall(&mut self) -> Result<Removed, Trouble> {
        let reply = self.run(&["uninstall", PACKAGE], None, T_UNINSTALL)?;
        Ok(parse_uninstall(&reply.text()))
    }

    /// What LiMo's log says it did with a pairing file since `since_secs`
    /// (the phone's clock), in its own words after `dev pair`: `taken`,
    /// `no_file`, `refused why=<word>`; or `failed error=<name>` when its
    /// receiver failed. None when it said nothing. Reads the log once, and
    /// only the tag `LiMo`.
    pub fn pair_word(&mut self, since_secs: i64) -> Result<Option<String>, Trouble> {
        let since = format!("{since_secs}.000");
        let reply = self.run(&["logcat", "-d", "-v", "time", "-t", &since, "-s", "LiMo:V"], None, T_SHORT)?;
        Ok(parse_pair_word(&reply.out()))
    }

    /// Follows the phone's log for LiMo's two tags, handing on each of
    /// their lines, until `limit` or until stopped. False when it was
    /// stopped by the limit.
    pub fn follow_log(&mut self, limit: Duration, on_line: &mut dyn FnMut(&str)) -> Result<bool, Trouble> {
        let mut args = vec!["-s".to_string(), self.serial.clone(), "logcat".to_string(), "-v".to_string(), "time".to_string(), "-s".to_string()];
        args.extend(LOG_TAGS.iter().map(|t| format!("{t}:V")));
        let mut keep = |line: &str| {
            if is_limo_line(line) {
                on_line(line);
            }
        };
        let reply =
            self.adb.run(Call { args, stdin: None, timeout: limit, on_line: Some(&mut keep) }).map_err(Trouble::Adb)?;
        if let Some(trouble) = trouble_in(&reply) {
            return Err(trouble);
        }
        Ok(!reply.timed_out)
    }
}

/// `dumpsys package`: the version name, and whether its flags say
/// DEBUGGABLE (None when no flags line was found).
pub fn parse_dump(text: &str) -> (Option<String>, Option<bool>) {
    let marker = format!("Package [{PACKAGE}]");
    let Some(start) = text.find(&marker) else { return (None, None) };
    let block = &text[start + marker.len()..];
    let block = &block[..block.find("Package [").unwrap_or(block.len())];
    let mut version = None;
    let mut debuggable = None;
    for line in block.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("versionName=") {
            version = Some(v.trim().to_string());
        }
        if t.starts_with("flags=[") || t.starts_with("pkgFlags=[") {
            let on = t.split_whitespace().any(|w| w == "DEBUGGABLE");
            debuggable = Some(debuggable.unwrap_or(false) || on);
        }
    }
    (version, debuggable)
}

/// What `run-as` answered.
pub fn run_as_answer(reply: &Reply) -> RunAs {
    let text = reply.text();
    let lower = text.to_ascii_lowercase();
    if reply.ok() && reply.out().trim().parse::<u32>().is_ok() {
        RunAs::Works
    } else if lower.contains("not debuggable") {
        RunAs::Release
    } else if lower.contains("unknown package") || lower.contains("is unknown") {
        RunAs::NotInstalled
    } else {
        RunAs::Broken(first_line(&text))
    }
}

/// `date +%s; date +%z`: unix seconds and seconds from UTC.
pub fn parse_clock(text: &str) -> Option<(i64, i32)> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let secs: i64 = lines.next()?.parse().ok()?;
    let zone = lines.next()?;
    Some((secs, parse_zone(zone)?))
}

/// "+0530" as 19800, "-0800" as -28800.
pub fn parse_zone(zone: &str) -> Option<i32> {
    let (sign, digits) = match zone.as_bytes().first()? {
        b'+' => (1, &zone[1..]),
        b'-' => (-1, &zone[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    if hours > 14 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

/// `adb install`: "Success", or Android's code.
pub fn parse_install(text: &str) -> Installed {
    if text.lines().any(|l| l.trim() == "Success") {
        return Installed::Done;
    }
    Installed::Refused { code: upper_code(text, "INSTALL_").unwrap_or_else(|| "unknown".to_string()), said: first_line(text) }
}

/// `adb uninstall`.
pub fn parse_uninstall(text: &str) -> Removed {
    if text.lines().any(|l| l.trim() == "Success") {
        Removed::Done
    } else if text.contains("DELETE_FAILED_DEVICE_POLICY_MANAGER") {
        Removed::StillAdmin
    } else if text.contains("DELETE_FAILED_OWNER_BLOCKED") {
        Removed::Blocked
    } else if text.contains("Unknown package") || text.contains("not installed") {
        Removed::NotInstalled
    } else {
        Removed::Refused(first_line(text))
    }
}

/// The first "PREFIX_WORDS" code in the text.
fn upper_code(text: &str, prefix: &str) -> Option<String> {
    let i = text.find(prefix)?;
    let code: String = text[i..].chars().take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_').collect();
    (code.len() > prefix.len()).then_some(code)
}

/// True for a log line of one of LiMo's two tags, in logcat's `time`
/// format: `11-20 21:42:03.123 I/LiMo    ( 1234): the message`.
pub fn is_limo_line(line: &str) -> bool {
    let Some(slash) = line.find('/') else { return false };
    let rest = &line[slash + 1..];
    let Some(open) = rest.find('(') else { return false };
    let tag = rest[..open].trim();
    LOG_TAGS.contains(&tag)
}

/// The last word LiMo's debug receiver logged about a pairing file: what
/// follows `dev pair ` (`taken`, `no_file`, `refused why=old_file`), or
/// `failed error=<name>` for `dev failed error=<name>`.
pub fn parse_pair_word(text: &str) -> Option<String> {
    text.lines().rev().filter(|line| is_limo_line(line)).find_map(|line| {
        let (_, message) = line.split_once("): ")?;
        let message = message.trim();
        message
            .strip_prefix("dev pair ")
            .map(str::to_string)
            .or_else(|| message.strip_prefix("dev failed ").map(|rest| format!("failed {rest}")))
    })
}

/// What to do after LiMo's word about a pairing file it did not take.
pub fn pair_advice(word: &str) -> String {
    let copy = "Copy what log shows, exactly.";
    let Some(why) = word.strip_prefix("refused why=") else {
        return match word {
            "taken" => format!("LiMo says it took it, yet its state does not hold the test phone. Run check. {copy}"),
            "no_file" => "LiMo found no pairing file when it was asked. Run setup again; if it says the same, copy what log shows, exactly.".to_string(),
            other if other.starts_with("failed") => format!("LiMo's receiver failed ({other}). {copy}"),
            other => format!("LiMo said \"dev pair {other}\", which this tool does not know. {copy}"),
        };
    };
    match why {
        "old_file" => "LiMo takes a pairing file only when it was made within an hour of the phone's own clock. Set the clock \
             that is wrong (this tool never changes the phone's), then run setup again."
            .to_string(),
        "no_child_name" => "LiMo found no child's name in the pairing file. This tool writes one (child_name), so the tool and \
             the app's debug build were built from different versions of the repository: build both from the same one, \
             then run setup again."
            .to_string(),
        "not_json" | "no_welcome" | "no_code_secret" | "no_made_at" | "code_secret_not_hex" | "code_secret_not_32_bytes" => {
            format!(
                "LiMo found the pairing file not in the form it takes ({why}), so the tool and the app's debug build do \
                 not agree on it: build both from the same version of the repository, then run setup again."
            )
        }
        "unreadable" | "not_deleted" => format!(
            "LiMo could not read or delete the pairing file ({why}). Run setup again; if it says the same, uninstall, \
             install and setup again."
        ),
        "engine" => format!("LiMo's engine refused the welcome, or its state could not be kept (engine). {copy}"),
        other => format!("LiMo refused the pairing file for a reason this tool does not know ({other}). {copy}"),
    }
}

/// Words for an install Android refused, with what to do.
pub fn install_advice(code: &str) -> &'static str {
    match code {
        "INSTALL_FAILED_UPDATE_INCOMPATIBLE" => {
            "The LiMo on the phone was signed with another key. Run uninstall first (it removes LiMo with everything it kept), then install again."
        }
        "INSTALL_FAILED_VERSION_DOWNGRADE" => {
            "The LiMo on the phone is newer than this file. Run uninstall first (it removes LiMo with everything it kept), then install again."
        }
        "INSTALL_FAILED_OLDER_SDK" => "The phone's Android is older than 8.0, the oldest LiMo runs on.",
        "INSTALL_FAILED_INSUFFICIENT_STORAGE" => "The phone has no room for it. Make room on the phone, then install again.",
        "INSTALL_FAILED_USER_RESTRICTED" => {
            "The phone refused an install over USB. Some phones ask for it to be allowed on the phone itself (\"Install via USB\" in Developer options, or a prompt on the screen): look at the phone, then install again. This tool never changes that setting."
        }
        "INSTALL_FAILED_ABORTED" | "INSTALL_FAILED_VERIFICATION_FAILURE" => {
            "The phone stopped the install, or asked about it on its screen. Look at the phone, then install again."
        }
        _ => "Look at the phone's screen for a prompt, then install again.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_read_whatever_their_state() {
        let text = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\n\
                    R5CN70\tdevice\nemulator-5554\toffline\nABC\tunauthorized\nXYZ\tno permissions; see [http://...]\n\n";
        let list = parse_devices(text);
        assert_eq!(list.len(), 4);
        assert_eq!(list[2].state, "unauthorized");
        assert_eq!(list[3].state, "no permissions");
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn one_phone_ready_is_picked_and_anything_else_is_said_plainly() {
        let line = |s: &str, st: &str| DeviceLine { serial: s.to_string(), state: st.to_string() };
        assert_eq!(pick(&[line("R5", "device")]), Ok("R5".to_string()));
        assert_eq!(pick(&[]), Err(PickProblem::NoPhone));
        assert_eq!(pick(&[line("R5", "unauthorized")]), Err(PickProblem::NotAllowed("R5".to_string())));
        assert_eq!(pick(&[line("R5", "offline")]), Err(PickProblem::Offline("R5".to_string())));
        assert!(matches!(pick(&[line("R5", "recovery")]), Err(PickProblem::Other(..))));
        let two = pick(&[line("R5", "device"), line("emulator-5554", "device")]).unwrap_err();
        assert!(two.to_string().contains("R5 device, emulator-5554 device"), "{two}");
        for problem in [PickProblem::NoPhone, PickProblem::NotAllowed("R5".to_string()), PickProblem::Offline("R5".to_string())] {
            assert!(problem.to_string().ends_with('.'), "{problem}");
        }
    }

    #[test]
    fn run_as_tells_a_debug_build_from_a_release_build() {
        assert_eq!(run_as_answer(&Reply::ok_with("10245\n")), RunAs::Works);
        assert_eq!(run_as_answer(&Reply::failed_with(1, "run-as: package not debuggable: ai.lucidloom.limo.child\n")), RunAs::Release);
        assert_eq!(run_as_answer(&Reply::failed_with(1, "run-as: Package 'ai.lucidloom.limo.child' is not debuggable\n")), RunAs::Release);
        assert_eq!(run_as_answer(&Reply::failed_with(1, "run-as: unknown package: ai.lucidloom.limo.child\n")), RunAs::NotInstalled);
        assert_eq!(
            run_as_answer(&Reply::failed_with(1, "run-as: couldn't stat /data/user/0/x: Permission denied\n")),
            RunAs::Broken("run-as: couldn't stat /data/user/0/x: Permission denied".to_string())
        );
    }

    #[test]
    fn the_clock_and_its_zone_are_read() {
        assert_eq!(parse_clock("1800000000\n+0530\n"), Some((1_800_000_000, 19_800)));
        assert_eq!(parse_clock("1800000000\r\n-0800\r\n"), Some((1_800_000_000, -28_800)));
        assert_eq!(parse_clock("1800000000\n+0000\n"), Some((1_800_000_000, 0)));
        assert_eq!(parse_clock("1800000000\n%z\n"), None);
        assert_eq!(parse_clock("soon\n+0530\n"), None);
        assert_eq!(parse_zone("+1500"), None);
        assert_eq!(parse_zone("+0575"), None);
    }

    #[test]
    fn the_dump_gives_the_version_and_the_debug_flag() {
        let text = "Packages:\n  Package [ai.lucidloom.limo.child] (abc):\n    versionName=0.1.0\n    flags=[ DEBUGGABLE HAS_CODE ALLOW_CLEAR_USER_DATA ]\n  Package [other] (d):\n    flags=[ HAS_CODE ]\n";
        assert_eq!(parse_dump(text), (Some("0.1.0".to_string()), Some(true)));
        let release = "  Package [ai.lucidloom.limo.child] (abc):\n    versionName=0.1.0\n    flags=[ HAS_CODE ]\n";
        assert_eq!(parse_dump(release), (Some("0.1.0".to_string()), Some(false)));
        assert_eq!(parse_dump("Unable to find package"), (None, None));
    }

    #[test]
    fn install_and_uninstall_answers_are_read() {
        assert_eq!(parse_install("Performing Streamed Install\nSuccess\n"), Installed::Done);
        assert_eq!(
            parse_install("adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package signatures do not match]"),
            Installed::Refused {
                code: "INSTALL_FAILED_UPDATE_INCOMPATIBLE".to_string(),
                said: "adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package signatures do not match]".to_string()
            }
        );
        assert_eq!(parse_uninstall("Success\n"), Removed::Done);
        assert_eq!(parse_uninstall("Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]"), Removed::StillAdmin);
        assert_eq!(parse_uninstall("Failure [DELETE_FAILED_OWNER_BLOCKED]"), Removed::Blocked);
        assert_eq!(parse_uninstall("Failure [DELETE_FAILED_INTERNAL_ERROR]\nUnknown package: x"), Removed::NotInstalled);
        assert!(matches!(parse_uninstall("Failure [DELETE_FAILED_INTERNAL_ERROR]"), Removed::Refused(_)));
    }

    #[test]
    fn only_limos_two_tags_are_kept_from_the_log() {
        assert!(is_limo_line("11-20 21:42:03.123 I/LiMo    ( 1234): tick took 3 ms"));
        assert!(is_limo_line("11-20 21:42:03.123 W/LiMoWatch( 1234): accessibility off"));
        assert!(!is_limo_line("11-20 21:42:03.123 I/LiMoOther( 1234): x"));
        assert!(!is_limo_line("--------- beginning of main"));
        assert!(!is_limo_line("11-20 21:42:03.123 I/ActivityManager( 99): LiMo/x(1)"));
    }

    #[test]
    fn the_state_is_read_exactly_and_its_answer_says_whether_one_follows() {
        let state = b"limo-phone-state\nformat 1\ncheck 0123456789abcdef\n{\"a\":1}";
        let mut answer = STATE_FOLLOWS.to_vec();
        answer.extend_from_slice(state);
        assert_eq!(parse_state_answer(&answer), Ok(Some(state.to_vec())));
        assert_eq!(parse_state_answer(NO_STATE), Ok(None));
        // An answer whose line ends were changed on the way is said so,
        // never handed on as the state.
        let changed: Vec<u8> = String::from_utf8(answer).unwrap().replace('\n', "\r\n").into_bytes();
        assert!(parse_state_answer(&changed).unwrap_err().to_string().contains("line ends changed"));
        assert!(parse_state_answer(b"limo-dev-none\r\n").is_err());
        // run-as refusing: its words come on the same stream.
        assert_eq!(
            parse_state_answer(b"run-as: package not debuggable: ai.lucidloom.limo.child\n"),
            Err(Trouble::Unexpected("run-as: package not debuggable: ai.lucidloom.limo.child".to_string()))
        );
        assert!(parse_state_answer(b"").is_err());
    }

    #[test]
    fn text_answers_read_the_same_with_carriage_returns() {
        // adb shell on Windows gives each line feed as CR LF.
        let crlf = |s: &str| s.replace('\n', "\r\n");
        assert_eq!(parse_devices(&crlf("List of devices attached\nR5CN70\tdevice\n\n")), parse_devices("List of devices attached\nR5CN70\tdevice\n\n"));
        assert_eq!(run_as_answer(&Reply::ok_with(&crlf("10245\n"))), RunAs::Works);
        assert_eq!(
            run_as_answer(&Reply::failed_with(1, &crlf("run-as: package not debuggable: ai.lucidloom.limo.child\n"))),
            RunAs::Release
        );
        let dump = "  Package [ai.lucidloom.limo.child] (abc):\n    versionName=0.1.0\n    flags=[ DEBUGGABLE HAS_CODE ]\n";
        assert_eq!(parse_dump(&crlf(dump)), parse_dump(dump));
        assert_eq!(parse_clock(&crlf("1800000000\n+0530\n")), Some((1_800_000_000, 19_800)));
        assert_eq!(parse_install(&crlf("Performing Streamed Install\nSuccess\n")), Installed::Done);
        assert_eq!(parse_uninstall(&crlf("Success\n")), Removed::Done);
        let log = "11-20 13:30:03.456 I/LiMo    ( 4242): dev pair taken\n";
        assert_eq!(parse_pair_word(&crlf(log)), Some("taken".to_string()));
        assert!(is_limo_line("11-20 13:30:03.456 I/LiMo    ( 4242): dev pair taken\r"));
        assert_eq!(first_line(&crlf("one\ntwo\n")), "one");
    }

    #[test]
    fn limos_own_word_about_a_pairing_file_is_read_and_explained() {
        let log = "--------- beginning of main\n\
                   11-20 13:29:01.000 I/LiMo    ( 4242): dev pair taken\n\
                   11-20 13:30:02.123 I/LiMo    ( 4242): host started\n\
                   11-20 13:30:03.456 I/LiMo    ( 4242): dev pair refused why=no_child_name\n";
        assert_eq!(parse_pair_word(log), Some("refused why=no_child_name".to_string()));
        assert_eq!(parse_pair_word("11-20 13:30:03.456 I/LiMo    ( 4242): dev failed error=IOException\n"), Some("failed error=IOException".to_string()));
        assert_eq!(parse_pair_word("11-20 13:30:03.456 I/Other   ( 4242): dev pair taken\n"), None);
        assert_eq!(parse_pair_word(""), None);
        assert!(pair_advice("refused why=no_child_name").contains("This tool writes one (child_name)"));
        assert!(pair_advice("refused why=old_file").contains("within an hour of the phone's own clock"));
        assert!(pair_advice("refused why=no_welcome").contains("(no_welcome)"));
        assert!(pair_advice("refused why=engine").contains("(engine)"));
        assert!(pair_advice("refused why=unreadable").contains("(unreadable)"));
        assert!(pair_advice("refused why=surprise").contains("(surprise)"));
        assert!(pair_advice("no_file").contains("no pairing file"));
        assert!(pair_advice("taken").contains("Run check."));
        assert!(pair_advice("failed error=IOException").contains("(failed error=IOException)"));
    }

    #[test]
    fn no_command_carries_anything_but_names_and_paths() {
        // The secrets cross only in the pairing file, through standard
        // input. Every command is fixed text.
        for command in [
            props_command(),
            listed_command(),
            dump_command(),
            probe_command(),
            look_command(),
            read_state_command(),
            write_pairing_command(),
            remove_pairing_command(),
            broadcast_command(PAIR_ACTION),
            broadcast_command(FORGET_ACTION),
            clock_command(),
        ] {
            assert!(!command.contains("code_secret") && !command.contains('{'), "{command}");
        }
        assert_eq!(
            write_pairing_command(),
            "run-as ai.lucidloom.limo.child sh -c 'umask 077 && mkdir -p files/limo-dev && cat > files/limo-dev/pairing.json.part && mv -f files/limo-dev/pairing.json.part files/limo-dev/pairing.json && wc -c < files/limo-dev/pairing.json'"
        );
        assert_eq!(
            broadcast_command(PAIR_ACTION),
            "am broadcast -a ai.lucidloom.limo.child.dev.PAIR -n ai.lucidloom.limo.child/.dev.DevReceiver --include-stopped-packages --receiver-foreground"
        );
    }
}
