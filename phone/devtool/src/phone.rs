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

/// LiMo's kept state, or exit code 3 when it has none.
pub fn read_state_command() -> String {
    format!("run-as {PACKAGE} sh -c 'test -e {STATE_FILE} || exit 3; cat {STATE_FILE}'")
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

    /// Runs one shell command on the phone, with no terminal, so that bytes
    /// cross as they are.
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
    pub fn read_state(&mut self) -> Result<Option<Vec<u8>>, Trouble> {
        let reply = self.shell(&read_state_command(), None)?;
        match reply.code {
            Some(0) => Ok(Some(reply.stdout)),
            Some(3) => Ok(None),
            _ => Err(Trouble::Unexpected(first_line(&reply.text()))),
        }
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
