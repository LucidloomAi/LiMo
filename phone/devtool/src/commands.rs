//! The eight commands, written against the [`Adb`] and [`Ui`] traits, so
//! that the tests run each one against a fake phone and read every line it
//! prints. Each says plainly what it did and what to do next.

use std::path::Path;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::adb::Adb;
use crate::apk::read_apk;
use crate::cli::{self, Command};
use crate::family::{self, Family, Plan};
use crate::pairing::pairing_bytes;
use crate::phone::{
    self, FORGET_ACTION, GUARDIAN_PACKAGE, Installed, OLDEST_SDK, PACKAGE, PAIR_ACTION, Phone, Removed, RunAs, Trouble,
};
use crate::places::{self, Places};
use crate::status::{self, Whose};
use crate::ui::{Hidden, Ui};

/// Exit code: done.
pub const DONE: i32 = 0;
/// Exit code: something went wrong while doing it.
pub const FAILED: i32 = 1;
/// Exit code: refused, with nothing changed; the message says what to do.
pub const REFUSED: i32 = 2;

/// `setup` refuses when the phone's clock and this PC's are this far apart,
/// in minutes: LiMo's debug build takes a pairing file only when it was made
/// within an hour of the phone's own clock.
pub const CLOCKS_APART_REFUSED_MINUTES: u64 = 50;
/// `setup` says so when they are this far apart, in minutes.
pub const CLOCKS_APART_NOTED_MINUTES: u64 = 5;

/// Everything a command works with.
pub struct Tool<'a> {
    /// adb, real or fake.
    pub adb: &'a mut dyn Adb,
    /// The terminal, real or fake.
    pub ui: &'a mut dyn Ui,
    /// Where the test family lives.
    pub places: Places,
    /// This PC's clock, unix seconds.
    pub clock: &'a dyn Fn() -> i64,
}

/// Runs one command line; the exit code.
pub fn run(tool: &mut Tool<'_>, argv: &[String]) -> i32 {
    let command = match cli::parse(argv) {
        Ok(command) => command,
        Err(problem) => {
            tool.ui.say(&problem);
            return REFUSED;
        }
    };
    match command {
        Command::Help => {
            tool.ui.say(&cli::usage(&tool.places.dir.display().to_string()));
            DONE
        }
        Command::Check => check(tool),
        Command::Install { apk } => install(tool, &apk),
        Command::Setup { child, minutes, block, allow, bedtime } => {
            setup(tool, Plan { child, minutes, block, allow, bedtime, utc_offset_secs: 0 })
        }
        Command::Code { minutes } => code(tool, minutes),
        Command::Status => status(tool),
        Command::Log { minutes } => log(tool, minutes),
        Command::Forget { phone_only: true, .. } => forget_phone_only(tool),
        Command::Forget { this_pc_only, .. } => forget(tool, this_pc_only),
        Command::Uninstall => uninstall(tool),
    }
}

const NOTHING_CHANGED: &str = "Nothing was changed on the phone.";
const PHONE_ONLY_NEXT: &str =
    "forget --phone-only, which wipes the family LiMo keeps on this phone and nothing else; then setup.";
const NOT_INSTALLED: &str =
    "LiMo's child form is not installed on this phone. Run install --apk PATH with the debug child form first.";
const RELEASE: &str = "LiMo on this phone is a release build. Android refuses run-as for a release build, so this tool \
     cannot reach LiMo's files, and a test family can go only on a debug build. Run uninstall (it removes LiMo with \
     everything it kept), then install the debug child form.";

/// adb made ready and the one phone picked; what went wrong said, and the
/// exit code, when not.
fn pick_phone<'p>(adb: &'p mut dyn Adb, ui: &mut dyn Ui, hint: Option<&str>) -> Result<Phone<'p>, i32> {
    match adb.ready() {
        Ok(about) => ui.say(&format!("adb: {about}")),
        Err(problem) => {
            ui.say(&problem);
            return Err(REFUSED);
        }
    }
    match Phone::pick(adb) {
        Ok(phone) => {
            ui.say(&format!("Phone: {}", phone.serial));
            Ok(phone)
        }
        Err(problem) => {
            ui.say(&problem.to_string());
            if let Some(hint) = hint {
                ui.say(hint);
            }
            ui.say(NOTHING_CHANGED);
            Err(REFUSED)
        }
    }
}

/// Says what went wrong while talking to the phone.
fn trouble(ui: &mut dyn Ui, what: &str, t: Trouble) -> i32 {
    ui.say(&format!("{what}: {t}"));
    FAILED
}

/// The phone's Android, said; refused when it is older than LiMo runs on.
fn android_ok(phone: &mut Phone<'_>, ui: &mut dyn Ui) -> Result<(), i32> {
    let props = phone.props().map_err(|t| trouble(ui, "The phone's Android could not be read", t))?;
    let sdk = props.sdk.map_or("unknown".to_string(), |s| s.to_string());
    ui.say(&format!("Android {} (API {sdk}), {} {}.", props.release, props.maker, props.model));
    if props.sdk.is_some_and(|s| s < OLDEST_SDK) {
        ui.say("LiMo needs Android 8.0 (API 26) or later, so it cannot run on this phone.");
        ui.say(NOTHING_CHANGED);
        return Err(REFUSED);
    }
    Ok(())
}

/// LiMo installed as a debug build, so that this tool can reach its files.
fn limo_reachable(phone: &mut Phone<'_>, ui: &mut dyn Ui) -> Result<(), i32> {
    if !phone.installed().map_err(|t| trouble(ui, "Whether LiMo is installed could not be read", t))? {
        ui.say(NOT_INSTALLED);
        ui.say(NOTHING_CHANGED);
        return Err(REFUSED);
    }
    match phone.probe().map_err(|t| trouble(ui, "run-as could not be tried", t))? {
        RunAs::Works => Ok(()),
        RunAs::Release => {
            ui.say(RELEASE);
            ui.say(NOTHING_CHANGED);
            Err(REFUSED)
        }
        RunAs::NotInstalled => {
            ui.say(NOT_INSTALLED);
            ui.say(NOTHING_CHANGED);
            Err(REFUSED)
        }
        RunAs::Broken(said) => {
            ui.say(&broken(&said));
            ui.say(NOTHING_CHANGED);
            Err(REFUSED)
        }
    }
}

fn broken(said: &str) -> String {
    format!(
        "Android refused run-as for LiMo on this phone: \"{said}\". Some makers' builds of Android break run-as even \
         for a debug build; this tool cannot reach LiMo's files on such a phone."
    )
}

/// The test family on this PC, said in a line.
fn family_line(family: &Option<Family>) -> String {
    match family {
        None => "The test family on this PC: none.".to_string(),
        Some(f) if f.phone.is_some() => format!("The test family on this PC: {}, with its test phone paired.", f.summary()),
        Some(f) => format!("The test family on this PC: {}, with no phone.", f.summary()),
    }
}

/// `check`: reads only.
fn check(tool: &mut Tool<'_>) -> i32 {
    let ui = &mut *tool.ui;
    ui.say("check reads only; it changes nothing.");
    let family = match family::read_places(&tool.places) {
        Ok(family) => family,
        Err(problem) => {
            ui.say(&problem);
            None
        }
    };
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    if let Err(code) = android_ok(&mut phone, ui) {
        ui.say(&family_line(&family));
        return code;
    }
    let installed = match phone.installed() {
        Ok(installed) => installed,
        Err(t) => return trouble(ui, "Whether LiMo is installed could not be read", t),
    };
    let next;
    if !installed {
        ui.say("LiMo's child form: not installed.");
        next = "install --apk PATH, with the debug child form.".to_string();
    } else {
        let (version, _) = phone.version_and_flag().unwrap_or((None, None));
        let version = version.map_or(String::new(), |v| format!(", version {v}"));
        match phone.probe() {
            Err(t) => return trouble(ui, "run-as could not be tried", t),
            Ok(RunAs::Release) => {
                ui.say(&format!("LiMo's child form: installed{version}, a release build. Android refuses run-as for it, so it cannot take a test family."));
                next = "uninstall, then install the debug child form.".to_string();
            }
            Ok(RunAs::NotInstalled) => {
                ui.say("LiMo's child form: Android lists it, but run-as does not know it.");
                next = "uninstall, then install again.".to_string();
            }
            Ok(RunAs::Broken(said)) => {
                ui.say(&format!("LiMo's child form: installed{version}."));
                ui.say(&broken(&said));
                next = "nothing this tool can do on this phone.".to_string();
            }
            Ok(RunAs::Works) => {
                ui.say(&format!("LiMo's child form: installed{version}, a debug build: run-as works, so this tool can reach LiMo's files."));
                let look = match phone.look() {
                    Ok(look) => look,
                    Err(t) => return trouble(ui, "LiMo's files could not be looked at", t),
                };
                let whose = if look.state {
                    match phone.read_state() {
                        Ok(Some(bytes)) => Some(status::whose(&Zeroizing::new(bytes), family.as_ref())),
                        Ok(None) => None,
                        Err(t) => return trouble(ui, "LiMo's state could not be read", t),
                    }
                } else {
                    None
                };
                next = match &whose {
                    None => {
                        ui.say("LiMo's state on the phone: none yet, so no family is on it.");
                        if family.is_some() {
                            "the test family on this PC is not on this phone. Run forget, then setup again.".to_string()
                        } else {
                            "setup --child NAME --minutes N.".to_string()
                        }
                    }
                    Some(Whose::Ours) => {
                        let child = family.as_ref().map_or("", |f| f.child.as_str());
                        ui.say(&format!("LiMo's state on the phone: it holds this PC's test family, {child}'s test phone."));
                        "status, log or code; or forget, to start again.".to_string()
                    }
                    Some(Whose::Other { device_id }) => {
                        ui.say(&format!("LiMo's state on the phone: it holds a family this PC did not make (device {device_id})."));
                        PHONE_ONLY_NEXT.to_string()
                    }
                    Some(Whose::Unreadable(problem)) => {
                        ui.say(&format!("LiMo's state on the phone cannot be read. {problem}"));
                        PHONE_ONLY_NEXT.to_string()
                    }
                };
                if look.pairing {
                    ui.say(
                        "A pairing file waits in LiMo's files: LiMo has not taken it. Is this a debug build with the \
                         development receiver? Run log to see what LiMo says.",
                    );
                }
            }
        }
    }
    ui.say(&family_line(&family));
    ui.say(&format!("Next: {next}"));
    DONE
}

/// `install --apk PATH`.
fn install(tool: &mut Tool<'_>, apk: &str) -> i32 {
    let ui = &mut *tool.ui;
    let path = Path::new(apk);
    if !path.is_file() {
        ui.say(&format!("There is no file {apk}. Build the debug child form first (phone/TESTING.md says how). Nothing was installed."));
        return REFUSED;
    }
    let facts = match read_apk(path) {
        Ok(facts) => facts,
        Err(problem) => {
            ui.say(&format!("{problem} Nothing was installed."));
            return REFUSED;
        }
    };
    if facts.package != PACKAGE {
        if facts.package == GUARDIAN_PACKAGE {
            ui.say(&format!(
                "This APK is LiMo's guardian form ({GUARDIAN_PACKAGE}), not the child form. This tool installs only the \
                 child form ({PACKAGE}). Nothing was installed."
            ));
        } else {
            ui.say(&format!("This APK is the app {}, not LiMo's child form ({PACKAGE}). Nothing was installed.", facts.package));
        }
        return REFUSED;
    }
    let version = facts.version_name.as_ref().map_or(String::new(), |v| format!(", version {v}"));
    if !facts.debuggable {
        ui.say(&format!(
            "This APK is LiMo's child form{version}, but a release build. A test family can go only on a debug build, \
             because Android refuses run-as for a release build. Build the debug child form. Nothing was installed."
        ));
        return REFUSED;
    }
    ui.say(&format!("The APK is LiMo's child form{version}, a debug build."));
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    if let Err(code) = android_ok(&mut phone, ui) {
        return code;
    }
    let was_there = match phone.installed() {
        Ok(was) => was,
        Err(t) => return trouble(ui, "Whether LiMo is installed could not be read", t),
    };
    ui.say("Installing; the phone may ask about it on its screen.");
    match phone.install(apk) {
        Err(t) => trouble(ui, "The install did not finish", t),
        Ok(Installed::Done) => {
            ui.say("Installed LiMo's child form on the phone.");
            if was_there {
                ui.say("LiMo was there already: it was replaced, and its files kept, so a test family on it stays.");
            }
            let _ = places::append(&tool.places, (tool.clock)(), "install put a debug build of LiMo's child form on the phone.");
            ui.say("Next: check, then setup --child NAME --minutes N.");
            DONE
        }
        Ok(Installed::Refused { code, said }) => {
            ui.say(&format!("Android refused the install: {said}"));
            ui.say(phone::install_advice(&code));
            FAILED
        }
    }
}

/// The PIN typed twice at a hidden prompt; refused, with the reason said,
/// when it cannot be had or the two differ.
fn ask_pin(ui: &mut dyn Ui) -> Result<Zeroizing<String>, i32> {
    let mut ask = |prompt: &str| match ui.hidden(prompt) {
        Hidden::Typed(typed) => Ok(typed),
        Hidden::NotATerminal => Err(
            "setup asks for the PIN at a prompt that does not show it, so it must run in a terminal, not with its \
             input from a file or a pipe.",
        ),
        Hidden::Nothing => Err("No PIN was typed."),
    };
    let pin = ask("Guardian PIN (6 to 8 digits, not shown): ");
    let pin = match pin {
        Ok(pin) => pin,
        Err(why) => {
            ui.say(&format!("{why} {NOTHING_CHANGED}"));
            return Err(REFUSED);
        }
    };
    let digits = pin.chars().count();
    if !(6..=8).contains(&digits) || !pin.chars().all(|c| c.is_ascii_digit()) {
        ui.say(&format!("A PIN is 6 to 8 digits, with nothing else in it. {NOTHING_CHANGED}"));
        return Err(REFUSED);
    }
    let again = match ui.hidden("The same PIN again: ") {
        Hidden::Typed(typed) => typed,
        _ => {
            ui.say(&format!("No PIN was typed the second time. {NOTHING_CHANGED}"));
            return Err(REFUSED);
        }
    };
    if *pin != *again {
        ui.say(&format!("The two PINs differ. {NOTHING_CHANGED}"));
        return Err(REFUSED);
    }
    Ok(pin)
}

/// "UTC+05:30".
fn zone_text(offset: i32) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let abs = offset.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", abs / 3_600, abs / 60 % 60)
}

/// Waits for LiMo to take the pairing file: the file gone and a state of
/// this device kept. True when it did.
fn wait_until_taken(phone: &mut Phone<'_>, device_id: &str) -> Result<bool, Trouble> {
    let tries = phone::ANSWER_WAIT.as_secs();
    for _ in 0..tries {
        phone.pause(Duration::from_secs(1));
        if taken(phone, device_id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True when the pairing file is gone and LiMo keeps a state of this
/// device. A state that cannot be read back is not confirmed: false, and
/// LiMo's own word decides what happens then.
fn taken(phone: &mut Phone<'_>, device_id: &str) -> Result<bool, Trouble> {
    let look = phone.look()?;
    if look.pairing || !look.state {
        return Ok(false);
    }
    let Some(bytes) = phone.read_state().ok().flatten().map(Zeroizing::new) else { return Ok(false) };
    let engine = limo_phone::PhoneEngine::load(&bytes);
    Ok(engine.view().is_some_and(|v| v.device_id == device_id))
}

/// What `setup` says when the phone may hold the test family but the tool
/// could not confirm it: the family on this PC is kept.
const KEPT_UNCONFIRMED: &str = "The test family on this PC is kept, so that this PC and the phone still match. Run \
     check, then status, to see what the phone holds. To start again, run forget: it wipes the test family from the \
     phone and from this PC.";

/// Undoes this PC's side of a setup the phone did not take, but only when
/// the phone keeps no state: setup pairs only a phone that keeps none, so a
/// state there now came from this pairing, and deleting the family on this
/// PC would leave the phone holding a family this PC no longer has. True
/// when it was undone.
fn undo_setup(tool_places: &Places, phone: &mut Phone<'_>, ui: &mut dyn Ui) -> bool {
    let _ = phone.remove_pairing();
    match phone.look() {
        Ok(look) if !look.state => {}
        Ok(_) => {
            ui.say("LiMo keeps a state on the phone now, which can only have come from this pairing.");
            ui.say(KEPT_UNCONFIRMED);
            return false;
        }
        Err(t) => {
            ui.say(&format!("Whether LiMo keeps a state on the phone now could not be read: {t}"));
            ui.say(KEPT_UNCONFIRMED);
            return false;
        }
    }
    match family::delete(tool_places) {
        Ok(()) => ui.say("The pairing file was removed from the phone, and the test family on this PC deleted again, so setup can run again."),
        Err(problem) => ui.say(&format!("The test family on this PC could not be deleted again: {problem}. Run forget --this-pc-only.")),
    }
    true
}

/// `setup`.
fn setup(tool: &mut Tool<'_>, mut plan: Plan) -> i32 {
    let ui = &mut *tool.ui;
    ui.say("setup is scaffolding until pairing over the network exists.");
    match family::read_places(&tool.places) {
        Ok(Some(family)) => {
            ui.say(&format!(
                "A test family is already on this PC: {}{}. setup makes one only once.",
                family.summary(),
                if family.phone.is_some() { ", with its test phone paired" } else { ", with no phone" }
            ));
            ui.say(
                "To start again, run forget: it asks LiMo on the phone to wipe the test family, checks that it did, and \
                 deletes the test family on this PC. Then setup again.",
            );
            ui.say("Nothing was changed.");
            return REFUSED;
        }
        Ok(None) => {}
        Err(problem) => {
            ui.say(&format!("{problem} Nothing was changed."));
            return FAILED;
        }
    }
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    if let Err(code) = android_ok(&mut phone, ui) {
        return code;
    }
    if let Err(code) = limo_reachable(&mut phone, ui) {
        return code;
    }
    match phone.look() {
        Err(t) => return trouble(ui, "LiMo's files could not be looked at", t),
        Ok(look) if look.state => {
            let held = match phone.read_state() {
                Ok(Some(bytes)) => match status::whose(&Zeroizing::new(bytes), None) {
                    Whose::Other { device_id } => format!("a family, device {device_id}"),
                    Whose::Unreadable(problem) => format!("a state it cannot read ({problem})"),
                    Whose::Ours => "a family".to_string(),
                },
                Ok(None) => "a state".to_string(),
                Err(t) => format!("a state this tool could not read back ({t})"),
            };
            ui.say(&format!(
                "LiMo on this phone already holds {held}. setup pairs only a phone that holds none. To start afresh, run \
                 forget --phone-only (it asks LiMo to wipe the state it keeps on the phone, and nothing else), then \
                 setup again."
            ));
            ui.say(NOTHING_CHANGED);
            return REFUSED;
        }
        Ok(_) => {}
    }
    let (phone_now, offset) = match phone.clock() {
        Ok(Some(clock)) => clock,
        Ok(None) => {
            ui.say("The phone's time zone could not be read (date +%z gave something else). Nothing was changed.");
            return FAILED;
        }
        Err(t) => return trouble(ui, "The phone's clock could not be read", t),
    };
    plan.utc_offset_secs = offset;
    ui.say(&format!("The phone's clock is in {}: the test family's days and bedtime follow it.", zone_text(offset)));
    let apart_minutes = (phone_now - (tool.clock)()).unsigned_abs() / 60;
    if apart_minutes >= CLOCKS_APART_REFUSED_MINUTES {
        ui.say(&format!(
            "The phone's clock and this PC's are {apart_minutes} minutes apart. LiMo's debug build takes a pairing file \
             only when it was made within an hour of the phone's own clock, and the codes this PC makes are for this \
             PC's day. Set the clock that is wrong (this tool never changes the phone's), then run setup again. \
             Nothing was changed."
        ));
        return REFUSED;
    }
    if apart_minutes >= CLOCKS_APART_NOTED_MINUTES {
        ui.say(&format!(
            "The phone's clock and this PC's are {apart_minutes} minutes apart. Near midnight, a code this PC makes may \
             be for another day than the phone's."
        ));
    }

    let pin = match ask_pin(ui) {
        Ok(pin) => pin,
        Err(code) => return code,
    };

    // This PC's side: the family, the child, the rules, the PIN, the phone.
    let now = (tool.clock)();
    if let Err(problem) = family::delete(&tool.places) {
        ui.say(&format!("An empty file left from before could not be removed: {problem}. Nothing was changed."));
        return FAILED;
    }
    let mut hub = match family::create(&tool.places) {
        Ok(hub) => hub,
        Err(problem) => {
            ui.say(&format!("{problem} Nothing was changed."));
            return FAILED;
        }
    };
    let made = family::make(&mut hub, &plan, &pin, now);
    drop(pin);
    let made = match made {
        Ok(made) => made,
        Err(problem) => {
            let _ = hub.close();
            let _ = family::delete(&tool.places);
            ui.say(&format!("{problem} Nothing was changed."));
            return FAILED;
        }
    };
    let bytes = pairing_bytes(&made.pairing, now, &plan.child);
    if let Err(problem) = hub.close() {
        ui.say(&format!("The test family's file did not close cleanly: {problem}"));
    }
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(problem) => {
            let _ = family::delete(&tool.places);
            ui.say(&format!("{problem} Nothing was changed."));
            return FAILED;
        }
    };
    ui.say(&format!(
        "Made the test family on this PC: {}, {} minutes a day, the away share at 100 percent (a phone with no master \
         has its whole day), and the PIN set.",
        plan.child, plan.minutes
    ));
    if !plan.block.is_empty() {
        ui.say(&format!("Blocked for {}: {}.", plan.child, plan.block.join(", ")));
    }
    if !plan.allow.is_empty() {
        ui.say(&format!("Always allowed, also under a lock: {}.", plan.allow.join(", ")));
    }
    if let Some(bedtime) = &plan.bedtime {
        ui.say(&format!("Screens off every day from {}.", family::bedtime_text(bedtime)));
    }
    ui.say("Paired one phone with it: the test phone.");

    // The phone's side: the pairing file through standard input, then the
    // broadcast, then reading back.
    if let Err(t) = phone.write_pairing(&bytes) {
        let code = trouble(ui, "The pairing file could not be written on the phone", t);
        undo_setup(&tool.places, &mut phone, ui);
        return code;
    }
    ui.say(&format!(
        "Wrote the pairing file into LiMo's own files on the phone ({} bytes, through adb's standard input).",
        bytes.len()
    ));
    drop(bytes);
    match phone.broadcast(PAIR_ACTION) {
        Ok(answer) => ui.say(&format!("Asked LiMo to take it. Android: {answer}")),
        Err(t) => {
            let code = trouble(ui, "LiMo could not be asked to take it", t);
            undo_setup(&tool.places, &mut phone, ui);
            return code;
        }
    }
    let device_id = made.pairing.device_id.clone();
    let mut took = match wait_until_taken(&mut phone, &device_id) {
        Ok(took) => took,
        Err(t) => {
            ui.say(&format!("Whether LiMo took it could not be read: {t}"));
            ui.say("Run check to see. If LiMo did not take it, run forget, then setup again.");
            return FAILED;
        }
    };
    if !took {
        // Removed, then looked at once more: LiMo may have taken it in the
        // last moment.
        let _ = phone.remove_pairing();
        took = taken(&mut phone, &device_id).unwrap_or(false);
    }
    if !took {
        // LiMo's own word, from its log since just before the broadcast.
        let word = phone.pair_word(phone_now.saturating_sub(2)).ok().flatten();
        if word.as_deref() == Some("taken") {
            // LiMo says it took it: this PC's side must stay, whatever this
            // tool could not read.
            ui.say(
                "LiMo says it took the test family (its log says: dev pair taken), but this tool could not read its \
                 state back to check that it holds the test phone.",
            );
            ui.say(KEPT_UNCONFIRMED);
            let _ = places::append(
                &tool.places,
                (tool.clock)(),
                "setup made a test family and paired one phone; LiMo said it took it, and its state could not be read back.",
            );
            return FAILED;
        }
        match word {
            Some(word) => {
                ui.say(&format!("LiMo did not take the test family. Its log says: dev pair {word}"));
                ui.say(&phone::pair_advice(&word));
            }
            None => ui.say(&format!(
                "LiMo did not take the test family within {} seconds, and its log says nothing of it. Is this debug \
                 build one with the development receiver? Run log in another terminal to see what LiMo says.",
                phone::ANSWER_WAIT.as_secs()
            )),
        }
        undo_setup(&tool.places, &mut phone, ui);
        return FAILED;
    }
    ui.say(&format!("LiMo took it: its state on the phone now holds {}'s test phone.", plan.child));
    let _ = places::append(
        &tool.places,
        (tool.clock)(),
        &format!(
            "setup made a test family with one child, {} minutes a day, {} blocked and {} always allowed apps{}, and paired one phone.",
            plan.minutes,
            plan.block.len(),
            plan.allow.len(),
            if plan.bedtime.is_some() { ", screens off at night" } else { "" }
        ),
    );
    ui.say(
        "Next: open LiMo on the phone and let it have what it asks for. status shows what it holds, code makes a one \
         time code, log follows what it says, and forget takes the test family away again.",
    );
    DONE
}

/// `code`.
fn code(tool: &mut Tool<'_>, minutes: u32) -> i32 {
    let ui = &mut *tool.ui;
    ui.say("code is scaffolding until the guardian's phone app exists.");
    let mut hub = match family::open(&tool.places) {
        Ok(Some(hub)) => hub,
        Ok(None) => {
            ui.say("There is no test family on this PC. Run setup first.");
            return REFUSED;
        }
        Err(problem) => {
            ui.say(&problem);
            return FAILED;
        }
    };
    let made = family::make_code(&mut hub, minutes, (tool.clock)());
    let closed = hub.close();
    let code = match made {
        Ok(code) => code,
        Err(problem) => {
            ui.say(&problem);
            return FAILED;
        }
    };
    ui.say(&format!("A one time code for {}'s test phone, worth {} minutes, for today only:", code.child, code.minutes));
    ui.say("");
    ui.say(&format!("    {}", *code.shown));
    ui.say("");
    ui.say(
        "It works once. On the phone's lock, choose \"Type a one time code\" and type its 8 digits. A code opens a lock \
         that a lack of time made; it does not open bedtime or a guardian's lock.",
    );
    ui.say("No phone was needed for this: the phone checks the code with the secret it shares with this PC.");
    if let Err(problem) = closed {
        ui.say(&format!("The test family's file did not close cleanly: {problem}"));
    }
    let _ = places::append(&tool.places, (tool.clock)(), &format!("code made a one time code of {} minutes.", code.minutes));
    DONE
}

/// `status`: reads only.
fn status(tool: &mut Tool<'_>) -> i32 {
    let ui = &mut *tool.ui;
    ui.say("status reads only; it changes nothing on the phone.");
    let family = match family::read_places(&tool.places) {
        Ok(family) => family,
        Err(problem) => {
            ui.say(&problem);
            None
        }
    };
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    if let Err(code) = limo_reachable(&mut phone, ui) {
        return code;
    }
    let bytes = match phone.read_state() {
        Ok(Some(bytes)) => Zeroizing::new(bytes),
        Ok(None) => {
            ui.say("LiMo on this phone keeps no state yet, so no family is on it. Run setup.");
            return DONE;
        }
        Err(t) => return trouble(ui, "LiMo's state could not be read", t),
    };
    let clock = match phone.clock() {
        Ok(clock) => clock,
        Err(t) => {
            ui.say(&format!("The phone's clock could not be read: {t}"));
            None
        }
    };
    for line in status::status_lines(&bytes, family.as_ref(), clock, (tool.clock)()) {
        ui.say(&line);
    }
    DONE
}

/// `log`: follows the phone's log.
fn log(tool: &mut Tool<'_>, minutes: u32) -> i32 {
    let ui = &mut *tool.ui;
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    ui.say(&format!(
        "Following LiMo's log on the phone (tags LiMo and LiMoWatch). Ctrl+C stops it; it stops by itself after {minutes} minutes."
    ));
    let limit = Duration::from_secs(u64::from(minutes) * 60);
    let mut show = |line: &str| ui.say(line);
    let ended = phone.follow_log(limit, &mut show);
    match ended {
        Ok(true) => {
            ui.say("The log ended: adb stopped following it, perhaps because the phone was unplugged. Run log again to go on.");
            DONE
        }
        Ok(false) => {
            ui.say(&format!("Stopped after {minutes} minutes. Run log again to go on."));
            DONE
        }
        Err(t) => trouble(ui, "The log could not be followed", t),
    }
}

/// Waits for LiMo to wipe its state. True when it did.
fn wait_until_wiped(phone: &mut Phone<'_>) -> Result<bool, Trouble> {
    for _ in 0..phone::ANSWER_WAIT.as_secs() {
        phone.pause(Duration::from_secs(1));
        if !phone.look()?.state {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Deletes this PC's side, and says so.
fn forget_here(tool_places: &Places, family: &Option<Family>, ui: &mut dyn Ui) -> Result<(), i32> {
    if family.is_none() && !tool_places.family_file().exists() {
        ui.say("There was no test family on this PC.");
        return Ok(());
    }
    match family::delete(tool_places) {
        Ok(()) => {
            ui.say("Deleted the test family on this PC.");
            Ok(())
        }
        Err(problem) => {
            ui.say(&format!("The test family on this PC could not be deleted: {problem}"));
            Err(FAILED)
        }
    }
}

/// `forget [--this-pc-only]`.
fn forget(tool: &mut Tool<'_>, this_pc_only: bool) -> i32 {
    let ui = &mut *tool.ui;
    if this_pc_only {
        ui.say("forget --this-pc-only deletes the test family on this PC, and asks nothing of any phone.");
    } else {
        ui.say("forget asks LiMo on the phone to wipe the test family, checks that it did, then deletes it on this PC.");
    }
    let family = match family::read_places(&tool.places) {
        Ok(family) => family,
        Err(problem) => {
            ui.say(&format!("{problem} Nothing was changed."));
            return FAILED;
        }
    };
    if this_pc_only {
        if let Err(code) = forget_here(&tool.places, &family, ui) {
            return code;
        }
        ui.say("The phone was not asked. If LiMo there still holds the test family, forget --phone-only wipes it.");
        let _ = places::append(&tool.places, (tool.clock)(), "forget deleted the test family on this PC alone.");
        return DONE;
    }
    let hint = "If that phone is gone for good, forget --this-pc-only deletes the test family on this PC alone.";
    let mut phone = match pick_phone(&mut *tool.adb, ui, Some(hint)) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    let installed = match phone.installed() {
        Ok(installed) => installed,
        Err(t) => return trouble(ui, "Whether LiMo is installed could not be read", t),
    };
    if installed {
        match phone.probe() {
            Err(t) => return trouble(ui, "run-as could not be tried", t),
            Ok(RunAs::Works) => {}
            Ok(RunAs::Release) => {
                ui.say(
                    "LiMo on this phone is a release build now, so this PC cannot reach its files to wipe the test \
                     family. Run uninstall (it removes LiMo with everything it kept), then forget again.",
                );
                ui.say("Nothing was changed.");
                return REFUSED;
            }
            Ok(RunAs::NotInstalled) => {}
            Ok(RunAs::Broken(said)) => {
                ui.say(&broken(&said));
                ui.say("Run uninstall (it removes LiMo with everything it kept), then forget again. Nothing was changed.");
                return REFUSED;
            }
        }
        let look = match phone.look() {
            Ok(look) => look,
            Err(t) => return trouble(ui, "LiMo's files could not be looked at", t),
        };
        if look.state {
            let whose = match phone.read_state() {
                Ok(Some(bytes)) => Some(status::whose(&Zeroizing::new(bytes), family.as_ref())),
                Ok(None) => None,
                Err(t) => {
                    let code = trouble(ui, "LiMo's state could not be read", t);
                    ui.say("Nothing was changed. forget --phone-only wipes the state LiMo keeps on the phone without reading it.");
                    return code;
                }
            };
            match whose {
                Some(Whose::Ours) => {
                    match phone.broadcast(FORGET_ACTION) {
                        Ok(answer) => ui.say(&format!("Asked LiMo to wipe the test family. Android: {answer}")),
                        Err(t) => return trouble(ui, "LiMo could not be asked to wipe the test family", t),
                    }
                    match wait_until_wiped(&mut phone) {
                        Ok(true) => ui.say("LiMo wiped it: its kept state is gone from the phone."),
                        Ok(false) => {
                            ui.say(&format!(
                                "LiMo did not wipe its state within {} seconds. The test family on this PC was kept, so \
                                 forget can run again. Run log in another terminal to see what LiMo says; uninstall \
                                 removes the state with LiMo.",
                                phone::ANSWER_WAIT.as_secs()
                            ));
                            return FAILED;
                        }
                        Err(t) => return trouble(ui, "Whether LiMo wiped it could not be read", t),
                    }
                }
                Some(Whose::Other { device_id }) => {
                    ui.say(&format!(
                        "LiMo on this phone holds a family this PC did not make (device {device_id}). forget leaves it \
                         alone: forget --phone-only wipes it, and nothing else."
                    ));
                    if family.is_some() {
                        ui.say("The test family on this PC was kept: forget --this-pc-only deletes it alone.");
                    }
                    ui.say("Nothing was changed.");
                    return REFUSED;
                }
                Some(Whose::Unreadable(problem)) => {
                    ui.say(&format!(
                        "LiMo's state on this phone cannot be read, so this PC cannot tell whose it is. {problem} \
                         forget --phone-only wipes it, and nothing else; then forget again."
                    ));
                    ui.say("Nothing was changed.");
                    return REFUSED;
                }
                None => ui.say("LiMo on the phone keeps no state: nothing of a family is on it."),
            }
        } else {
            ui.say("LiMo on the phone keeps no state: nothing of a family is on it.");
        }
        if look.pairing {
            match phone.remove_pairing() {
                Ok(()) => ui.say("Removed a pairing file LiMo had not taken."),
                Err(t) => return trouble(ui, "A pairing file LiMo had not taken could not be removed", t),
            }
        }
    } else {
        ui.say("LiMo is not on this phone, so nothing of the test family is left on it.");
    }
    if let Err(code) = forget_here(&tool.places, &family, ui) {
        return code;
    }
    let _ = places::append(&tool.places, (tool.clock)(), "forget wiped the test family on the phone and deleted it on this PC.");
    ui.say("Next: setup, to make a new test family.");
    DONE
}

/// `forget --phone-only`: LiMo's state on the phone wiped, whoever's it is,
/// and this PC's test family left as it is. The one way to clean a phone
/// that holds a family this PC does not have, with nothing touched but
/// LiMo's own state.
fn forget_phone_only(tool: &mut Tool<'_>) -> i32 {
    let ui = &mut *tool.ui;
    ui.say(
        "forget --phone-only asks LiMo on the phone to wipe the family it keeps, whoever's it is, checks that it did, \
         and leaves the test family on this PC as it is.",
    );
    let family = family::read_places(&tool.places).ok().flatten();
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    match phone.installed() {
        Ok(true) => {}
        Ok(false) => {
            ui.say("LiMo is not on this phone, so there is nothing of LiMo's to wipe. Nothing was changed.");
            return DONE;
        }
        Err(t) => return trouble(ui, "Whether LiMo is installed could not be read", t),
    }
    match phone.probe() {
        Err(t) => return trouble(ui, "run-as could not be tried", t),
        Ok(RunAs::Works) | Ok(RunAs::NotInstalled) => {}
        Ok(RunAs::Release) => {
            ui.say(
                "LiMo on this phone is a release build, so this PC cannot reach its files, and the debug receiver that \
                 wipes the state is not in it. uninstall removes LiMo with everything it kept.",
            );
            ui.say(NOTHING_CHANGED);
            return REFUSED;
        }
        Ok(RunAs::Broken(said)) => {
            ui.say(&broken(&said));
            ui.say("uninstall removes LiMo with everything it kept.");
            ui.say(NOTHING_CHANGED);
            return REFUSED;
        }
    }
    let look = match phone.look() {
        Ok(look) => look,
        Err(t) => return trouble(ui, "LiMo's files could not be looked at", t),
    };
    if look.state {
        // What it holds is said before it goes; it goes whatever this says.
        let held = match phone.read_state() {
            Ok(Some(bytes)) => match status::whose(&Zeroizing::new(bytes), family.as_ref()) {
                Whose::Ours => {
                    let child = family.as_ref().map_or("", |f| f.child.as_str());
                    format!("this PC's test family, {child}'s test phone")
                }
                Whose::Other { device_id } => format!("a family this PC did not make (device {device_id})"),
                Whose::Unreadable(problem) => format!("a state it cannot read ({problem})"),
            },
            Ok(None) => "no state any more".to_string(),
            Err(t) => format!("a state this tool could not read back ({t})"),
        };
        ui.say(&format!("LiMo on the phone holds {held}."));
        match phone.broadcast(FORGET_ACTION) {
            Ok(answer) => ui.say(&format!("Asked LiMo to wipe it. Android: {answer}")),
            Err(t) => return trouble(ui, "LiMo could not be asked to wipe it", t),
        }
        match wait_until_wiped(&mut phone) {
            Ok(true) => ui.say("LiMo wiped it: its kept state is gone from the phone."),
            Ok(false) => {
                ui.say(&format!(
                    "LiMo did not wipe its state within {} seconds. Run log in another terminal to see what LiMo says; \
                     uninstall removes the state with LiMo.",
                    phone::ANSWER_WAIT.as_secs()
                ));
                return FAILED;
            }
            Err(t) => return trouble(ui, "Whether LiMo wiped it could not be read", t),
        }
    } else {
        ui.say("LiMo on the phone keeps no state: there is nothing to wipe.");
    }
    if look.pairing {
        match phone.remove_pairing() {
            Ok(()) => ui.say("Removed a pairing file LiMo had not taken."),
            Err(t) => return trouble(ui, "A pairing file LiMo had not taken could not be removed", t),
        }
    }
    if family.is_some() {
        ui.say("The test family on this PC is still there: forget --this-pc-only deletes it, and setup needs it gone.");
    } else {
        ui.say("Next: setup, to make a new test family.");
    }
    let _ = places::append(&tool.places, (tool.clock)(), "forget --phone-only wiped LiMo's state on the phone.");
    DONE
}

/// `uninstall`.
fn uninstall(tool: &mut Tool<'_>) -> i32 {
    let ui = &mut *tool.ui;
    ui.say("uninstall removes LiMo from the phone, with everything it kept there.");
    let mut phone = match pick_phone(&mut *tool.adb, ui, None) {
        Ok(phone) => phone,
        Err(code) => return code,
    };
    match phone.installed() {
        Ok(true) => {}
        Ok(false) => {
            ui.say("LiMo's child form is not installed on this phone. Nothing was changed.");
            return DONE;
        }
        Err(t) => return trouble(ui, "Whether LiMo is installed could not be read", t),
    }
    let removed = match phone.uninstall() {
        Ok(removed) => removed,
        Err(t) => return trouble(ui, "The uninstall did not finish", t),
    };
    match removed {
        Removed::Done | Removed::NotInstalled => {
            if phone.installed().unwrap_or(false) {
                ui.say("Android said it removed LiMo, but still lists it. Run check.");
                return FAILED;
            }
            ui.say("Removed LiMo from the phone, with everything it kept.");
            let still_here = family::read_places(&tool.places).ok().flatten().is_some();
            if still_here {
                ui.say("The test family on this PC is still there: run forget to delete it too. It will find LiMo gone from the phone.");
            }
            let _ = places::append(&tool.places, (tool.clock)(), "uninstall removed LiMo from the phone.");
            DONE
        }
        Removed::StillAdmin => {
            ui.say(
                "Android will not remove LiMo while it is a device admin on this phone (its protection). Switch the \
                 protection off on the phone (Settings, Security, Device admin apps), then run uninstall again. This \
                 tool never changes it.",
            );
            ui.say(NOTHING_CHANGED);
            REFUSED
        }
        Removed::Blocked => {
            ui.say("A Device Owner on this phone blocks removing LiMo. This tool never touches a Device Owner.");
            ui.say(NOTHING_CHANGED);
            REFUSED
        }
        Removed::Refused(said) => {
            ui.say(&format!("Android refused to remove LiMo: {said}"));
            FAILED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_are_said_as_utc_and_hours() {
        assert_eq!(zone_text(19_800), "UTC+05:30");
        assert_eq!(zone_text(-28_800), "UTC-08:00");
        assert_eq!(zone_text(0), "UTC+00:00");
    }
}
