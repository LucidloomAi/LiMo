//! The steps of the run, one a command: each says first what it will do to
//! the phone, then each thing it sends as it sends it (`> COMMAND`), what
//! came back, and what that means.

#![forbid(unsafe_code)]

use std::path::Path;
use std::time::{Duration, Instant};

use crate::apk::read_apk;
use crate::cable::{Answer, Cable, CableError, Connector, OpenProblem, Via};
use crate::drivers::{Report, words};
use crate::echo::Echo;
use crate::key::{Key, SHOWN_AS};
use crate::phone::{self, Ask, DoorStatus, PACKAGE};
use crate::plan::{StepName, does};
use crate::secret::Sample;
use crate::ui::Ui;

/// The test app as Gradle builds it, from this proof's folder.
pub const APK_PATH: &str = "phone/app/build/outputs/apk/release/app-release.apk";
/// How long the door is given to say it took a write.
const DOOR_WAIT: Duration = Duration::from_secs(10);
/// How long the test app is given to connect through the reverse.
const LOOPBACK_WAIT: Duration = Duration::from_secs(30);
/// The most one loopback connection may last.
const LOOPBACK_LIMIT: Duration = Duration::from_secs(60);

/// How a step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// It did what it says.
    Done,
    /// It did not; what happened was said.
    Failed,
    /// It did nothing on the phone, and said why.
    Refused,
}

impl Exit {
    /// The program's exit code.
    pub fn code(&self) -> i32 {
        match self {
            Exit::Done => 0,
            Exit::Failed => 1,
            Exit::Refused => 2,
        }
    }
}

fn title(step: StepName) -> &'static str {
    match step {
        StepName::Drivers => "Drivers: what Windows bound to the phone",
        StepName::List => "List: the phones each route sees",
        StepName::Connect => "Connect: the phone's prompt, and what it is",
        StepName::Install => "Install the test app",
        StepName::WriteSecret => "Write a secret into the test app's guarded door",
        StepName::Start => "Start the test app",
        StepName::Reverse => "Reverse: the phone's loopback carried to this PC",
        StepName::Uninstall => "Remove the test app",
        StepName::DebuggingOff => "Switch USB debugging off",
    }
}

fn header(ui: &mut dyn Ui, step: StepName, via: Option<Via>) {
    match via {
        Some(via) => ui.say(&format!("{}, through {}.", title(step), via.words())),
        None => ui.say(&format!("{}.", title(step))),
    }
    ui.say(&format!("What this does to the phone: {}", does(step, via)));
    ui.say("");
}

fn open(connector: &dyn Connector, via: Via, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, Exit> {
    connector.open(via, ui).map_err(|problem| {
        ui.say(&problem.to_string());
        match problem {
            OpenProblem::AdbServerRunning | OpenProblem::AdbMissing | OpenProblem::TwoPhones(_) => Exit::Refused,
            _ => Exit::Failed,
        }
    })
}

fn finish(cable: Box<dyn Cable>, ui: &mut dyn Ui, exit: Exit) -> Exit {
    for line in cable.close() {
        ui.say(&line);
    }
    exit
}

fn say_answer(ui: &mut dyn Ui, answer: &Result<Answer, CableError>) {
    match answer {
        Ok(a) => {
            for line in phone::lines(&a.text) {
                ui.say(&format!("  {line}"));
            }
            if let Some(code) = a.exit {
                ui.say(&format!("  (exit code {code})"));
            }
        }
        Err(e) => ui.say(&format!("  Not answered: {e}.")),
    }
}

/// Sends one command, saying it first and what came back after.
fn ask(cable: &dyn Cable, ui: &mut dyn Ui, ask: Ask) -> Result<Answer, CableError> {
    ui.say(&format!("> {}", ask.command()));
    let answer = cable.shell(&ask);
    say_answer(ui, &answer);
    answer
}

/// What the door says now, asked without saying anything.
fn door_now(cable: &dyn Cable) -> Option<DoorStatus> {
    cable.shell(&Ask::DoorStatus).ok().and_then(|a| phone::door_status(&a.text))
}

/// Waits until the door's status passes `ready`, asking twice a second.
fn door_until(cable: &dyn Cable, wait: Duration, ready: impl Fn(&DoorStatus) -> bool) -> Option<DoorStatus> {
    let deadline = Instant::now() + wait;
    loop {
        let now = door_now(cable);
        if now.as_ref().is_some_and(&ready) || Instant::now() >= deadline {
            return now;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `drivers`: Windows' lists, read only.
pub fn drivers(report: Result<Report, String>, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Drivers, None);
    match report {
        Ok(report) => {
            for line in words(&report) {
                ui.say(&line);
            }
            ui.say("");
            ui.say("Run this again after five minutes with the phone still in: Windows may fetch a driver by itself in the meantime.");
            Exit::Done
        }
        Err(why) => {
            ui.say(&format!("Windows' lists could not be read: {why}."));
            Exit::Failed
        }
    }
}

/// `list`: what each route sees, opening nothing for use.
pub fn list(connector: &dyn Connector, via: Option<Via>, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::List, via);
    let routes = match via {
        Some(via) => vec![via],
        None => vec![Via::Libusb, Via::Nusb],
    };
    let mut exit = Exit::Done;
    for via in routes {
        match connector.seen(via) {
            Ok(seen) if seen.is_empty() => ui.say(&format!("{} sees no phone with USB debugging on.", via.word())),
            Ok(seen) => {
                for line in seen {
                    ui.say(&format!("{} sees: {line}", via.word()));
                }
            }
            Err(why) => {
                ui.say(&format!("{}: {why}.", via.word()));
                exit = Exit::Failed;
            }
        }
    }
    exit
}

/// `connect`: the prompt, then what the phone is.
pub fn connect(connector: &dyn Connector, via: Via, key: Option<&Key>, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Connect, Some(via));
    match (via, key) {
        (Via::Adb, _) => ui.say(
            "Google's adb uses its own key, in .android\\adbkey in your user folder. If the phone has allowed it before, \
             it asks nothing.",
        ),
        (_, Some(key)) => {
            ui.say(&format!(
                "This program's adb key: {SHOWN_AS} ({}).",
                if key.new { "made just now: the phone has never seen it" } else { "kept from an earlier run" }
            ));
            ui.say(&format!("Its fingerprint, as the phone shows it: {}", key.fingerprint));
            ui.say(
                "Unlock the phone and keep it in hand. Once it shows \"Allow USB debugging?\", adb_client waits 10 \
                 seconds for the answer: tick \"Always allow from this computer\" (on a PC that is not yours, leave \
                 it unticked), then tap Allow. If the 10 seconds pass first, tap Allow anyway and run connect again.",
            );
        }
        (_, None) => ui.say("This program's adb key could not be made, so this route cannot connect."),
    }
    ui.say("");
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    let model = ask(cable.as_ref(), ui, Ask::Model).map(|a| phone::first_line(&a.text)).unwrap_or_default();
    let version = ask(cable.as_ref(), ui, Ask::AndroidVersion).map(|a| phone::first_line(&a.text)).unwrap_or_default();
    let sdk = ask(cable.as_ref(), ui, Ask::Sdk).map(|a| phone::first_line(&a.text)).unwrap_or_default();
    let adb = ask(cable.as_ref(), ui, Ask::AdbEnabled).map(|a| phone::first_line(&a.text)).unwrap_or_default();
    let installed = ask(cable.as_ref(), ui, Ask::IsInstalled).map(|a| phone::lists_the_test_app(&a.text));
    ui.say("");
    let read = !model.is_empty() && !version.is_empty();
    if read {
        ui.say(&format!(
            "Connected: a {model}, Android {version} (API {sdk}); USB debugging setting: {}; the test app: {}.",
            if adb.is_empty() { "not read".to_string() } else { adb },
            match installed {
                Ok(true) => "installed",
                Ok(false) => "not installed",
                Err(_) => "not known",
            }
        ));
    } else {
        ui.say("Connected, but the phone's properties did not come back: see the answers above.");
    }
    finish(cable, ui, if read { Exit::Done } else { Exit::Failed })
}

/// What to do about Android's refusal of an install.
pub fn install_advice(words: &str) -> &'static str {
    let has = |code: &str| words.contains(code);
    if has("INSTALL_FAILED_UPDATE_INCOMPATIBLE") {
        "The test app on the phone is signed with another key (built on another PC, or its throwaway key was made \
         again): run uninstall, then install again."
    } else if has("INSTALL_FAILED_VERSION_DOWNGRADE") {
        "The phone holds a newer build of the test app: run uninstall, then install again."
    } else if has("INSTALL_FAILED_USER_RESTRICTED") || has("INSTALL_FAILED_ABORTED") || has("canceled by user") {
        "The phone refused an install over USB, or a question on the phone was answered no. Some phones have \
         \"Install via USB\" in Developer options, which must be on; on newer Samsung phones Auto Blocker blocks \
         installs over the cable; a Play Protect question answered no ends the same way. Write down what the phone \
         showed."
    } else if has("INSTALL_FAILED_VERIFICATION_FAILURE") {
        "Android's own check of apps (Play Protect) refused it. Write down its words exactly."
    } else if has("INSTALL_FAILED_INSUFFICIENT_STORAGE") {
        "The phone has no room for it."
    } else if has("INSTALL_FAILED_OLDER_SDK") {
        "The phone's Android is older than 8.0, which the test app needs."
    } else if has("INSTALL_PARSE_FAILED_NO_CERTIFICATES") {
        "The APK is not signed: build it again as README.md says."
    } else {
        "Android's own words are above: write them down."
    }
}

/// `install`: the test app, checked on this PC first.
pub fn install(connector: &dyn Connector, via: Via, apk: &Path, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Install, Some(via));
    let facts = match read_apk(apk) {
        Ok(facts) => facts,
        Err(problem) => {
            ui.say(&format!("The test app's APK was looked for at {}. {problem}", apk.display()));
            ui.say("Build it first: in phone\\, .\\gradlew.bat assembleRelease (README.md, \"Building\").");
            return Exit::Refused;
        }
    };
    if facts.package != PACKAGE {
        ui.say(&format!(
            "That APK is {}, not the test app ({PACKAGE}). This program installs nothing but its own test app.",
            facts.package
        ));
        return Exit::Refused;
    }
    if facts.debuggable {
        ui.say("That APK is a debug build. This proof installs the release build: in phone\\, .\\gradlew.bat assembleRelease.");
        return Exit::Refused;
    }
    let size = std::fs::metadata(apk).map(|m| m.len()).unwrap_or(0);
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    ui.say(&format!(
        "> install the APK ({size} bytes: {PACKAGE}, version {}, a release build), replacing it if it is there",
        facts.version_name.as_deref().unwrap_or("not given")
    ));
    let started = Instant::now();
    let answer = cable.install(apk);
    let took = started.elapsed().as_secs_f64();
    let exit = match &answer {
        Ok(words) if words == "Success" => {
            ui.say(&format!("  Android said: Success, in {took:.1} seconds."));
            Exit::Done
        }
        Ok(words) => {
            ui.say(&format!("  Android said: {words}"));
            ui.say(install_advice(words));
            Exit::Failed
        }
        Err(e) => {
            ui.say(&format!("  Not answered: {e}."));
            Exit::Failed
        }
    };
    let installed = ask(cable.as_ref(), ui, Ask::IsInstalled).map(|a| phone::lists_the_test_app(&a.text));
    ui.say(match installed {
        Ok(true) => "The phone lists the test app now.",
        Ok(false) => "The phone does not list the test app.",
        Err(_) => "Whether the phone lists the test app could not be read.",
    });
    ui.say("Look at the phone: if it asked anything (Play Protect, or a question of its maker's), write down its words and what was tapped.");
    let exit = if exit == Exit::Done && installed != Ok(true) { Exit::Failed } else { exit };
    finish(cable, ui, exit)
}

/// `write-secret`: a sample secret through the door, from standard input.
pub fn write_secret(connector: &dyn Connector, via: Via, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::WriteSecret, Some(via));
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    let before = ask(cable.as_ref(), ui, Ask::DoorStatus).ok().and_then(|a| phone::door_status(&a.text));
    let Some(before) = before else {
        ui.say("The test app's door does not answer: is the test app installed? Run install first.");
        return finish(cable, ui, Exit::Failed);
    };
    let sample = Sample::new();
    ui.say(&format!(
        "The sample secret: {} bytes, check {} (the secret itself is never shown, and is wiped from memory after).",
        sample.len(),
        sample.check()
    ));
    ui.say(&format!("> {}   (the {} bytes on its standard input, which then ends)", Ask::DoorWrite.command(), sample.len()));
    let written = cable.shell_with_input(&Ask::DoorWrite, sample.bytes());
    say_answer(ui, &written);
    if let Ok(Answer { exit: None, .. }) = &written {
        ui.say("  (this route does not carry the command's exit code; the door's own answer follows)");
    }
    ui.say(&format!("> {}   (asked until the door has seen the write, at most {} seconds)", Ask::DoorStatus.command(), DOOR_WAIT.as_secs()));
    let after = door_until(cable.as_ref(), DOOR_WAIT, |s| s.takes > before.takes);
    let exit = match after {
        Some(s) if s.takes > before.takes => {
            ui.say(&format!(
                "  The door says: {}, {} bytes, check {}, written by user id {} (2000 is the adb shell), why {}.",
                s.secret, s.bytes, s.check, s.caller_uid, s.why
            ));
            if s.secret == "taken" && s.bytes as usize == sample.len() && s.check == sample.check() {
                ui.say("The test app took exactly what this PC sent: every byte, none changed on the way.");
                Exit::Done
            } else if s.secret == "taken" {
                ui.say(&format!(
                    "The test app took something else than was sent: {} bytes with check {} were sent. The bytes \
                     were changed on the way (a carriage return added before the line feed would make it one byte \
                     more). That is a result: write it down.",
                    sample.len(),
                    sample.check()
                ));
                Exit::Failed
            } else {
                ui.say("The door refused the write: see its word above.");
                Exit::Failed
            }
        }
        _ => {
            ui.say("  The door did not see any write within the time: the secret did not reach the test app.");
            Exit::Failed
        }
    };
    drop(sample);
    finish(cable, ui, exit)
}

/// `start`: the test app's screen.
pub fn start(connector: &dyn Connector, via: Via, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Start, Some(via));
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    let answer = ask(cable.as_ref(), ui, Ask::Start);
    let exit = match answer {
        Ok(a) if phone::started(&a.text) => {
            ui.say("Android started the test app's screen. Look at the phone: \"LiMo spike 07\" should be open.");
            Exit::Done
        }
        _ => {
            ui.say("Android did not start the test app's screen: see its words above. Is it installed?");
            Exit::Failed
        }
    };
    finish(cable, ui, exit)
}

/// `reverse`: the phone's own loopback carried to an echo server here.
/// `port` is the link's port in the run; the tests give 0 for any free one.
pub fn reverse(connector: &dyn Connector, via: Via, port: u16, ui: &mut dyn Ui) -> Exit {
    reverse_waiting(connector, via, port, LOOPBACK_WAIT, ui)
}

/// `reverse`, waiting `wait` for the test app's connection.
pub fn reverse_waiting(connector: &dyn Connector, via: Via, port: u16, wait: Duration, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Reverse, Some(via));
    let echo = match Echo::bind(port) {
        Ok(echo) => echo,
        Err(why) => {
            ui.say(&why);
            ui.say("Nothing was asked of the phone. Stop whatever listens there, then run this again.");
            return Exit::Refused;
        }
    };
    let port = echo.port();
    ui.say(&format!("The echo server listens on 127.0.0.1:{port} on this PC."));
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    let before = door_now(cable.as_ref());
    if before.is_none() {
        ui.say("The test app's door does not answer: is the test app installed? Run install first.");
        return finish(cable, ui, Exit::Failed);
    }
    let loopbacks_before = before.map(|s| s.loopbacks).unwrap_or(0);
    ui.say(&format!("> reverse tcp:{port} tcp:{port}   (the phone's 127.0.0.1:{port} carried to this PC's)"));
    if let Err(e) = cable.reverse(port) {
        ui.say(&format!("  Not set: {e}."));
        return finish(cable, ui, Exit::Failed);
    }
    ui.say("  Set.");
    let started = ask(cable.as_ref(), ui, Ask::StartLoopback(port));
    let mut exit = Exit::Done;
    if !started.as_ref().is_ok_and(|a| phone::started(&a.text)) {
        ui.say("Android did not start the test app's screen: see its words above.");
        exit = Exit::Failed;
    } else {
        ui.say(&format!("Waiting up to {} seconds for the test app's connection.", wait.as_secs()));
        match echo.serve_one(wait, LOOPBACK_LIMIT) {
            Ok(Some(echoed)) => ui.say(&format!(
                "The phone's connection came through the cable: {} bytes came and went back, in {} ms.",
                echoed.bytes,
                echoed.took.as_millis()
            )),
            Ok(None) => {
                ui.say("No connection came: the reverse did not carry the test app's connection, or the app did not try.");
                exit = Exit::Failed;
            }
            Err(why) => {
                ui.say(&why);
                exit = Exit::Failed;
            }
        }
        ui.say(&format!("> {}   (asked until the test app has finished its run)", Ask::DoorStatus.command()));
        match door_until(cable.as_ref(), Duration::from_secs(10), |s| s.loopbacks > loopbacks_before) {
            Some(s) if s.loopbacks > loopbacks_before && s.loopback == "ok" => ui.say(&format!(
                "  The test app says: every byte came back ({} bytes, {} ms).",
                s.loopback_bytes, s.loopback_ms
            )),
            Some(s) if s.loopbacks > loopbacks_before => {
                ui.say(&format!("  The test app says: {}, why {}.", s.loopback, s.loopback_why));
                exit = Exit::Failed;
            }
            _ => {
                ui.say("  The test app did not say it finished a run.");
                exit = Exit::Failed;
            }
        }
    }
    ui.say(&format!("> reverse --remove tcp:{port}"));
    match cable.reverse_remove(port) {
        Ok(()) => ui.say("  Removed: nothing on the phone reaches this PC through the cable any more."),
        Err(e) => {
            ui.say(&format!("  Not removed: {e}. It ends anyway when this step lets go of the phone."));
            exit = Exit::Failed;
        }
    }
    finish(cable, ui, exit)
}

/// `uninstall`: the test app, and only it.
pub fn uninstall(connector: &dyn Connector, via: Via, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::Uninstall, Some(via));
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    ui.say(&format!("> uninstall {PACKAGE}"));
    let answer = cable.uninstall();
    match &answer {
        Ok(words) => ui.say(&format!("  Android said: {words}")),
        Err(e) => ui.say(&format!("  Not answered: {e}.")),
    }
    let listed = ask(cable.as_ref(), ui, Ask::IsInstalled).map(|a| phone::lists_the_test_app(&a.text));
    let exit = match listed {
        Ok(false) => {
            ui.say("The test app is gone from the phone.");
            Exit::Done
        }
        Ok(true) => {
            ui.say("The phone still lists the test app.");
            Exit::Failed
        }
        Err(_) => {
            ui.say("Whether the phone still lists the test app could not be read.");
            Exit::Failed
        }
    };
    finish(cable, ui, exit)
}

/// `debugging-off`: the last step.
pub fn debugging_off(connector: &dyn Connector, via: Via, ui: &mut dyn Ui) -> Exit {
    header(ui, StepName::DebuggingOff, Some(via));
    let answer = ui.ask("Type OFF and press Enter to switch USB debugging off; anything else stops here:");
    if !answer.is_some_and(|a| a.eq_ignore_ascii_case("off")) {
        ui.say("Stopped: nothing was sent to the phone, and USB debugging stays as it is.");
        return Exit::Refused;
    }
    let cable = match open(connector, via, ui) {
        Ok(cable) => cable,
        Err(exit) => return exit,
    };
    let _ = ask(cable.as_ref(), ui, Ask::AdbEnabled);
    let switched = ask(cable.as_ref(), ui, Ask::DebuggingOff);
    match &switched {
        Ok(_) => ui.say("  The phone answered before the link ended."),
        Err(CableError::Lost(_)) => ui.say("  The link ended as the setting changed, as expected."),
        Err(_) => ui.say("  No answer came; whether the setting changed is read below."),
    }
    for line in cable.close() {
        ui.say(&line);
    }
    std::thread::sleep(Duration::from_secs(3));
    let exit = match connector.seen(via) {
        Ok(seen) if seen.is_empty() => {
            ui.say("USB debugging looks off: this route no longer sees the phone's debugging interface.");
            Exit::Done
        }
        Ok(_) => {
            ui.say(
                "This route still sees a debugging interface: USB debugging may still be on. Look on the phone \
                 (Settings, Developer options) and write down what it shows.",
            );
            Exit::Failed
        }
        Err(why) => {
            ui.say(&format!("Whether this route still sees the phone could not be read: {why}."));
            Exit::Failed
        }
    };
    ui.say(
        "The cable link has ended. To reach the phone over the cable again, a person switches USB debugging on \
         again on the phone (Settings, Developer options, USB debugging). Opening the test app by hand shows what \
         it reads of the setting.",
    );
    exit
}
