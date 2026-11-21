//! What `limo install`, `limo stop-for-upgrade` and `limo uninstall` decide,
//! over a seam ([`Machine`]) that the real PC (`win/machine.rs`) and the
//! tests' fake PC both implement, so that every step's logic is tested
//! without elevation.
//!
//! An installer copies the files and calls `limo.exe`; all the real work is
//! here. In order:
//!
//! 1. `limo stop-for-upgrade`, from a copy outside the program folder,
//!    before any file is copied: the service is stopped and every LiMo
//!    program running from the program folder (the helpers, the console)
//!    is ended, so that its files can be replaced. Nothing to do on a PC
//!    without LiMo.
//! 2. The installer copies the programs into `%ProgramFiles%\LiMo`.
//! 3. `limo install`, from the program folder: a new install, or over an
//!    existing one (an upgrade, or the same version again), keeping the
//!    family's file and everything in it.
//!
//! Its uninstaller calls `limo uninstall` before it removes the program
//! folder, and goes on only when that returns 0: when a family exists, the
//! guardian password is asked for and checked through the console's door, by
//! signing in (or in the family's file itself when the service does not
//! run), before anything is removed, unless a guardian allowed the removal
//! in LiMo's settings a moment before.
//!
//! Exit codes, as for every command: 0 done, 1 failed (said, and in the
//! setup log), 2 refused, with nothing changed. Each step is said, and
//! written with its time to the setup log, which holds no secret.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use limo_hub::PasswordOutcome;
use serde::{Deserialize, Serialize};

use crate::close::path_is_inside;
use crate::console::{ProblemKind, Reply, Request};
use crate::os::ProcessSeen;
use crate::protocol::Secret;
use crate::settings::{DATA_FOLDER_SDDL, Places, SERVICE_NAME, read_settings, settings_text};

/// The console's program, beside `limo.exe`.
pub const CONSOLE_EXE: &str = "limo-console.exe";
/// The phone app the console installs on a child's phone, in the program
/// folder.
pub const PHONE_APP: &str = r"phone\limo-child.apk";
/// The third party notices, in the program folder.
pub const NOTICES: &str = "notices";
/// The console's shortcut, in the Start menu of every account.
pub const SHORTCUT_FILE: &str = "LiMo settings.lnk";
/// What the shortcut says of itself.
pub const SHORTCUT_DESCRIPTION: &str = "LiMo’s settings, for a guardian: children, rules, time and phones.";
/// What was installed, in the data folder: the version, so that the next
/// install can say what it replaces.
pub const RECORD_FILE: &str = "installed.json";
/// The setup log, in the data folder.
pub const SETUP_LOG: &str = "setup.log";
/// The setup log of an uninstall that deleted the data folder, in the
/// temporary folder of the account that ran it.
pub const SETUP_LOG_AFTER_DELETE: &str = "LiMo setup.log";

/// The link's port, as `limo.exe` listens on it.
pub const LINK_PORT: u16 = 47810;
/// Multicast DNS: the announcement, and the questions phones ask.
pub const MDNS_PORT: u16 = 5353;
/// Where a firewall rule lets traffic in from: the networks this PC is on.
pub const RULE_REMOTE_ADDRESSES: &str = "LocalSubnet";
/// The firewall's group of LiMo's rules, as its window shows it.
pub const FIREWALL_GROUP: &str = "LiMo";
/// The firewall's profiles the rules apply on: Private, and Public too,
/// since Windows calls a new Wi-Fi network Public until someone says
/// otherwise, and a family's often stays so. Never the Domain profile.
pub const RULE_PROFILES: Profiles = Profiles { private: true, public: true, domain: false };

/// How many more times the uninstall knocks, a second apart, on a door that
/// did not answer.
pub const DOOR_TRIES: u32 = 4;

/// How many tries the uninstall gives a person typing the password in one
/// run; the family's file counts every wrong one and sets its waits.
pub const PASSWORD_TRIES: u32 = 3;

/// The firewall's profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profiles {
    /// Networks a person called private, such as home.
    pub private: bool,
    /// Networks called public, which Windows calls a new one.
    pub public: bool,
    /// An organisation's domain network.
    pub domain: bool,
}

/// A rule's protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// TCP: the link.
    Tcp,
    /// UDP: the announcement.
    Udp,
}

/// One inbound rule of Windows' firewall: allow, for one program, one
/// local port, from the local subnet, on [`RULE_PROFILES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallRule {
    /// The name the firewall's window shows, by which LiMo finds it again.
    pub name: String,
    /// What it is for, in words.
    pub description: String,
    /// TCP or UDP.
    pub protocol: Protocol,
    /// The local port.
    pub port: u16,
    /// The one program it lets in.
    pub program: PathBuf,
}

impl FirewallRule {
    /// What the rule allows, in words.
    pub fn summary(&self) -> String {
        let protocol = match self.protocol {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
        };
        format!(
            "{protocol} port {} inbound, to {} only, from the local subnet only, on Private and Public networks",
            self.port,
            self.program.display()
        )
    }
}

/// LiMo's two rules, for the installed `limo.exe`.
pub fn firewall_rules(program: &Path) -> Vec<FirewallRule> {
    vec![
        FirewallRule {
            name: format!("LiMo, the family’s phones (TCP {LINK_PORT})"),
            description: format!(
                "Lets the family’s paired phones reach LiMo on this PC over the home network: TCP port {LINK_PORT}, \
                 to limo.exe only, from the local subnet only. Added by LiMo’s installer, removed with LiMo."
            ),
            protocol: Protocol::Tcp,
            port: LINK_PORT,
            program: program.to_path_buf(),
        },
        FirewallRule {
            name: format!("LiMo, found by the family’s phones (UDP {MDNS_PORT})"),
            description: format!(
                "Lets the family’s phones find LiMo on this PC on the home network (multicast DNS): UDP port {MDNS_PORT}, \
                 to limo.exe only, from the local subnet only. Added by LiMo’s installer, removed with LiMo."
            ),
            protocol: Protocol::Udp,
            port: MDNS_PORT,
            program: program.to_path_buf(),
        },
    ]
}

/// The service as Windows has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSeen {
    /// Stopped, running, or on its way (start pending, stop pending...).
    pub state: ServiceRun,
    /// The program it runs, when that could be read.
    pub exe: Option<PathBuf>,
}

/// Whether the service runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceRun {
    /// Stopped.
    Stopped,
    /// Running.
    Running,
    /// Any other state, by its name.
    Other(String),
}

/// A folder's owner and access list, as SDDL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSeen {
    /// The owner: `SY`, `BA` or a SID.
    pub owner: String,
    /// The access list, such as `D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`.
    pub dacl: String,
}

/// What a data folder that is there already is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderVerdict {
    /// Made by SYSTEM or an administrator, with LiMo's protection.
    Limos,
    /// Made by SYSTEM or an administrator, but its access list was changed.
    AccessChanged,
    /// Not LiMo's: why.
    NotLimos(String),
}

/// Whether a folder that is there is LiMo's. A standard account can make
/// folders in ProgramData, so one made by anyone but SYSTEM or the
/// Administrators group is never used: its maker could read the family's
/// file, or have put files in it.
pub fn judge_folder(seen: Option<&FolderSeen>) -> FolderVerdict {
    let Some(seen) = seen else {
        return FolderVerdict::NotLimos("its owner and access list could not be read".to_string());
    };
    if !matches!(seen.owner.as_str(), "SY" | "BA" | "S-1-5-18" | "S-1-5-32-544") {
        return FolderVerdict::NotLimos(format!(
            "it was made by the account {}, not by SYSTEM or an administrator",
            seen.owner
        ));
    }
    if same_access(&seen.dacl, DATA_FOLDER_SDDL) { FolderVerdict::Limos } else { FolderVerdict::AccessChanged }
}

/// Two access lists the same: protected, with the same entries in any
/// order. Windows may add the auto inherited mark (`PAI`) to a protected
/// list.
fn same_access(seen: &str, wanted: &str) -> bool {
    let split = |sddl: &str| -> Option<(String, Vec<String>)> {
        let body = sddl.strip_prefix("D:")?;
        let at = body.find('(').unwrap_or(body.len());
        let (flags, aces) = body.split_at(at);
        let mut entries: Vec<String> = aces.split(')').filter(|a| !a.is_empty()).map(|a| format!("{a})")).collect();
        entries.sort();
        Some((flags.to_string(), entries))
    };
    match (split(seen), split(wanted)) {
        (Some((flags, entries)), Some((_, wanted))) => flags.contains('P') && entries == wanted,
        _ => false,
    }
}

/// What an install leaves in the data folder about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    /// The version of LiMo installed, such as "0.1.0".
    pub version: String,
}

/// A version's three numbers, when it has them.
pub fn version_numbers(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    let numbers = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(numbers)
}

/// The door of LiMo's settings, as the uninstall finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoorOpen {
    /// It said hello.
    Open {
        /// The running LiMo's version.
        version: String,
    },
    /// It refused this account, in its own words (a child's account).
    Refused(String),
    /// What answers is not LiMo's service: why. Nothing was sent.
    NotTheService(String),
    /// Nothing answered: why.
    Absent(String),
}

/// Whether an uninstall keeps the family's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyFile {
    /// Kept, for LiMo installed again.
    Keep,
    /// Deleted for good.
    Delete,
}

/// The PC, as the installer's steps see it. Every change goes through one
/// of these calls; the real PC's are in `win/machine.rs`, the tests' are a
/// fake that records them.
pub trait Machine {
    /// True when this process runs elevated.
    fn elevated(&mut self) -> bool;
    /// This program's own file.
    fn this_exe(&mut self) -> PathBuf;
    /// The program and data folders.
    fn places(&mut self) -> Places;
    /// The Start menu folder of every account.
    fn shortcut_folder(&mut self) -> PathBuf;
    /// A folder for the setup log of an uninstall that deleted the data
    /// folder: the running account's own temporary folder.
    fn temp_folder(&mut self) -> PathBuf;
    /// The time now, for the setup log.
    fn timestamp(&mut self) -> String;
    /// The time now, in seconds since 1970, for the family's file.
    fn wall_secs(&mut self) -> i64;

    /// Whether a file or folder is there.
    fn exists(&mut self, path: &Path) -> bool;
    /// A text file's text, None when it cannot be read.
    fn read_text(&mut self, path: &Path) -> Option<String>;
    /// Writes a text file, replacing it.
    fn write_text(&mut self, path: &Path, text: &str) -> Result<(), String>;
    /// Adds text at the end of a file, making it when it is not there.
    fn append_text(&mut self, path: &Path, text: &str) -> Result<(), String>;
    /// Copies a file, replacing what is at `to`.
    fn copy_file(&mut self, from: &Path, to: &Path) -> Result<(), String>;
    /// Makes a folder, and the folders above it.
    fn make_folder(&mut self, path: &Path) -> Result<(), String>;
    /// Removes a file.
    fn remove_file(&mut self, path: &Path) -> Result<(), String>;
    /// Removes a folder and everything in it, trying for a few seconds while
    /// a stopping process lets go of its files.
    fn remove_folder(&mut self, path: &Path) -> Result<(), String>;
    /// Makes the data folder with LiMo's protection from its first moment.
    fn make_protected_folder(&mut self, path: &Path) -> Result<(), String>;
    /// Sets LiMo's protection on a folder that is there.
    fn protect_folder(&mut self, path: &Path) -> Result<(), String>;
    /// A folder's owner and access list.
    fn folder_seen(&mut self, path: &Path) -> Option<FolderSeen>;

    /// The service, None when it is not installed.
    fn service(&mut self) -> Result<Option<ServiceSeen>, String>;
    /// Creates the service: LocalSystem, automatic, restarted after a
    /// failure, running `exe service`.
    fn create_service(&mut self, exe: &Path) -> Result<(), String>;
    /// The same, for a service that is there.
    fn configure_service(&mut self, exe: &Path) -> Result<(), String>;
    /// Starts the service.
    fn start_service(&mut self) -> Result<(), String>;
    /// Stops the service and waits until it has.
    fn stop_service(&mut self) -> Result<(), String>;
    /// Deletes the service and waits until it is gone.
    fn delete_service(&mut self) -> Result<(), String>;

    /// The programs running from a folder, never this process.
    fn running_from(&mut self, folder: &Path) -> Vec<ProcessSeen>;
    /// Ends a process, checking first that it is the one seen.
    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String>;
    /// Waits a moment, for programs that are closing.
    fn pause(&mut self, ms: u64);

    /// Whether a rule of Windows' firewall has this name.
    fn firewall_has_rule(&mut self, name: &str) -> Result<bool, String>;
    /// Adds a rule, as [`FirewallRule`] says, in the group [`FIREWALL_GROUP`].
    fn add_firewall_rule(&mut self, rule: &FirewallRule) -> Result<(), String>;
    /// Removes one rule of this name.
    fn remove_firewall_rule(&mut self, name: &str) -> Result<(), String>;

    /// Makes a shortcut at `at` to `target`.
    fn make_shortcut(&mut self, at: &Path, target: &Path, description: &str) -> Result<(), String>;

    /// Knocks on the door of LiMo's settings, and says hello.
    fn door_open(&mut self) -> DoorOpen;
    /// A request through the open door, a second after the last.
    fn door_ask(&mut self, request: Request) -> Result<Reply, String>;
    /// Closes the door's connection, when one is open.
    fn door_close(&mut self);

    /// Whether the family's file has a family, read from the file itself
    /// (the service does not run).
    fn file_has_family(&mut self) -> Result<bool, String>;
    /// The guardian password checked in the family's file itself, with its
    /// waits.
    fn file_check_password(&mut self, password: &Secret, now: i64) -> Result<PasswordOutcome, String>;

    /// Asks a person for a secret, which does not show; None when nothing
    /// could be read (no console).
    fn ask_secret(&mut self, prompt: &str) -> Option<Secret>;
    /// Asks a person for a line; None when nothing could be read.
    fn ask_line(&mut self, prompt: &str) -> Option<String>;
    /// Says something to the person at the PC.
    fn say(&mut self, text: &str);
}

/// One run: what it said, for the setup log.
struct Run<'a, M: Machine> {
    m: &'a mut M,
    lines: Vec<String>,
}

impl<'a, M: Machine> Run<'a, M> {
    fn new(m: &'a mut M) -> Run<'a, M> {
        Run { m, lines: Vec::new() }
    }

    /// Says a step, and keeps it for the log.
    fn say(&mut self, text: &str) {
        self.m.say(text);
        let time = self.m.timestamp();
        let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        self.lines.push(format!("{time} {flat}"));
    }

    /// Writes the run's lines to the setup log: in the data folder while it
    /// is LiMo's, otherwise in the account's temporary folder. Says where.
    fn finish(self, code: i32) -> i32 {
        let places = self.m.places();
        let mut text = self.lines.join("\n");
        text.push_str(&format!("\n{} The run ended with exit code {code}.\n\n", self.m.timestamp()));
        let in_data = places.data_dir.join(SETUP_LOG);
        let data_is_limos = self.m.exists(&places.data_dir)
            && matches!(judge_folder(self.m.folder_seen(&places.data_dir).as_ref()), FolderVerdict::Limos);
        let written = if data_is_limos && self.m.append_text(&in_data, &text).is_ok() {
            Some(in_data)
        } else {
            let in_temp = self.m.temp_folder().join(SETUP_LOG_AFTER_DELETE);
            self.m.append_text(&in_temp, &text).ok().map(|()| in_temp)
        };
        match written {
            Some(path) => self.m.say(&format!("The steps of this run are written in {}.", path.display())),
            None => self.m.say("The steps of this run could not be written to a log."),
        }
        code
    }
}

const NEEDS_ADMINISTRATOR: &str = "needs an administrator: run it from an administrator terminal, or let the installer ask Windows for one.";

/// Whether `path` is inside `folder`.
fn inside(path: &Path, folder: &Path) -> bool {
    path_is_inside(&path.to_string_lossy(), &folder.to_string_lossy())
}

/// `limo install [--dev]`: a new install, or over one that is there,
/// keeping the family's file. `dev` switches the test cap on.
pub fn install(m: &mut impl Machine, dev: bool) -> i32 {
    let mut run = Run::new(m);
    let code = install_steps(&mut run, dev);
    run.finish(code)
}

/// What the data folder is, at the start of an install.
enum DataFolder {
    Absent,
    Kept(FolderVerdict),
}

fn install_steps<M: Machine>(run: &mut Run<'_, M>, dev: bool) -> i32 {
    let version = crate::windows_version();
    let kind = if dev { "a test install, with the test cap" } else { "install" };
    run.say(&format!("LiMo for Windows {version}: {kind}."));
    if !run.m.elevated() {
        run.say(&format!("Installing LiMo {NEEDS_ADMINISTRATOR} Nothing was changed."));
        return 2;
    }
    let places = run.m.places();
    let me = run.m.this_exe();
    let from_installed = inside(&me, &places.install_dir);
    let service = match run.m.service() {
        Ok(service) => service,
        Err(problem) => {
            run.say(&format!(
                "The service control manager could not be asked whether LiMo is installed: {problem}. Nothing was changed."
            ));
            return 1;
        }
    };
    let data = if run.m.exists(&places.data_dir) {
        let seen = run.m.folder_seen(&places.data_dir);
        DataFolder::Kept(judge_folder(seen.as_ref()))
    } else {
        DataFolder::Absent
    };
    if let DataFolder::Kept(FolderVerdict::NotLimos(why)) = &data {
        run.say(&format!(
            "The data folder {} is there, but it is not LiMo’s: {why}. A standard account can make folders in ProgramData, \
             so LiMo uses only one made by SYSTEM or an administrator. Remove it from an administrator’s account, then \
             install again. Nothing was changed.",
            places.data_dir.display()
        ));
        return 2;
    }

    // What this replaces, and a version never replaced by an older one.
    let recorded = match &data {
        DataFolder::Kept(_) => run
            .m
            .read_text(&places.data_dir.join(RECORD_FILE))
            .and_then(|text| serde_json::from_str::<InstallRecord>(&text).ok())
            .map(|record| record.version),
        DataFolder::Absent => None,
    };
    if let Some(old) = &recorded
        && let (Some(old_numbers), Some(new_numbers)) = (version_numbers(old), version_numbers(&version))
        && old_numbers > new_numbers
    {
        run.say(&format!(
            "LiMo {old} is installed, newer than this LiMo {version}, and its family’s file may be newer than this one \
             can read. Use the installer of LiMo {old} or newer. Nothing was changed."
        ));
        return 2;
    }
    let replaces = match (&recorded, service.is_some()) {
        (Some(old), true) if *old == version => format!("LiMo {old} is installed: it is installed again over itself."),
        (Some(old), true) => format!("This replaces LiMo {old} with LiMo {version}."),
        (None, true) => format!(
            "This replaces an earlier LiMo whose version was not recorded (a test install made before LiMo kept it) with LiMo {version}."
        ),
        (Some(old), false) => format!("The data folder of an earlier LiMo {old} is here, with the family’s file: it is used again."),
        (None, false) if matches!(data, DataFolder::Kept(_)) => {
            "The data folder of an earlier LiMo is here, with the family’s file: it is used again.".to_string()
        }
        (None, false) => format!("A new install of LiMo {version}."),
    };
    run.say(&replaces);

    // Nothing of LiMo may run from the program folder while its files are
    // replaced and its service changed.
    let had_service = service.is_some();
    if let Err(problem) = stop_limo(run, &places, service.as_ref()) {
        run.say(&format!("Install failed: {problem}."));
        if had_service {
            restart_after_failure(run);
        }
        return 1;
    }

    match install_files(run, &places, &me, from_installed) {
        Ok(()) => {}
        Err(problem) => {
            run.say(&format!("Install failed: {problem}. Run the installer again."));
            if had_service {
                restart_after_failure(run);
            }
            return 1;
        }
    }
    if let Err(problem) = data_folder(run, &places, &data) {
        run.say(&format!("Install failed: {problem}. Run the installer again."));
        return 1;
    }
    if let Err(problem) = settings_and_record(run, &places, dev, &version) {
        run.say(&format!("Install failed: {problem}. Run the installer again."));
        return 1;
    }
    let exe = places.installed_exe();
    let service_done = if had_service {
        run.m.configure_service(&exe).map(|()| {
            format!(
                "Updated the service {SERVICE_NAME}: it runs {} as LocalSystem, starts with Windows, and is restarted after a failure.",
                exe.display()
            )
        })
    } else {
        run.m.create_service(&exe).map(|()| {
            format!(
                "Created the service {SERVICE_NAME}: it runs {} as LocalSystem, starts with Windows, and is restarted after a failure.",
                exe.display()
            )
        })
    };
    match service_done {
        Ok(words) => run.say(&words),
        Err(problem) => {
            run.say(&format!("Install failed: the service could not be {}: {problem}. Run the installer again.", if had_service {
                "updated"
            } else {
                "created"
            }));
            return 1;
        }
    }

    // The steps a family can live without for a while: said loudly when
    // not done, and the exit code says so.
    let mut not_done = 0;
    not_done += add_firewall_rules(run, &exe);
    not_done += make_shortcut(run, &places);
    match run.m.start_service() {
        Ok(()) => run.say(&format!(
            "Started the service {SERVICE_NAME}. It starts a helper in each signed in session that needs one, so nothing is \
             installed for each account."
        )),
        Err(problem) => {
            run.say(&format!(
                "NOT DONE: the service could not be started: {problem}. It starts with Windows; or start it with sc.exe start {SERVICE_NAME}."
            ));
            not_done += 1;
        }
    }
    if not_done == 0 {
        run.say(&format!("LiMo {version} is installed."));
        0
    } else {
        run.say(&format!("LiMo {version} is installed, but {not_done} step(s) were not done: see above."));
        1
    }
}

/// After a failed install over one that was there: the service started
/// again, so that the family is not left unlimited.
fn restart_after_failure<M: Machine>(run: &mut Run<'_, M>) {
    match run.m.start_service() {
        Ok(()) => run.say(&format!("The service {SERVICE_NAME} is started again, as it was.")),
        Err(problem) => run.say(&format!(
            "The service {SERVICE_NAME} could not be started again: {problem}. Start it with sc.exe start {SERVICE_NAME}."
        )),
    }
}

/// Stops the service when it runs, and ends every LiMo program running
/// from the program folder (helpers, consoles), so that the folder's files
/// can be replaced.
fn stop_limo<M: Machine>(run: &mut Run<'_, M>, places: &Places, service: Option<&ServiceSeen>) -> Result<(), String> {
    if let Some(service) = service
        && service.state != ServiceRun::Stopped
    {
        run.m.stop_service().map_err(|problem| format!("the service could not be stopped: {problem}"))?;
        run.say(&format!("Stopped the service {SERVICE_NAME}."));
    }
    // The helpers end by themselves when the service stops; a moment for
    // them, then the rest are ended.
    for _ in 0..4 {
        if run.m.running_from(&places.install_dir).is_empty() {
            break;
        }
        run.m.pause(500);
    }
    let left = run.m.running_from(&places.install_dir);
    for process in &left {
        match run.m.end_process(process) {
            Ok(()) => run.say(&format!("Ended {} (pid {}) in session {}.", process.name, process.pid, process.session_id)),
            Err(problem) => run.say(&format!("{} (pid {}) could not be ended: {problem}.", process.name, process.pid)),
        }
    }
    let still = run.m.running_from(&places.install_dir);
    if still.is_empty() {
        Ok(())
    } else {
        let names: Vec<String> = still.iter().map(|p| format!("{} (pid {})", p.name, p.pid)).collect();
        Err(format!("still running from {}: {}", places.install_dir.display(), names.join(", ")))
    }
}

/// The programs: copied from a build folder for a test install, or already
/// put in place by the installer. Says which parts are missing.
fn install_files<M: Machine>(run: &mut Run<'_, M>, places: &Places, me: &Path, from_installed: bool) -> Result<(), String> {
    if from_installed {
        run.say(&format!("The programs are in {}, put there by the installer.", places.install_dir.display()));
    } else {
        run.m.make_folder(&places.install_dir).map_err(|e| format!("making {}: {e}", places.install_dir.display()))?;
        run.m.copy_file(me, &places.installed_exe()).map_err(|e| format!("copying the program: {e}"))?;
        run.say(&format!("Copied the program to {}.", places.installed_exe().display()));
        if let Some(beside) = me.parent().map(|folder| folder.join(CONSOLE_EXE))
            && run.m.exists(&beside)
        {
            let to = places.install_dir.join(CONSOLE_EXE);
            run.m.copy_file(&beside, &to).map_err(|e| format!("copying the console: {e}"))?;
            run.say(&format!("Copied the console to {}.", to.display()));
        }
    }
    for (part, without) in [
        (CONSOLE_EXE, "there is no console, and no shortcut is made"),
        (PHONE_APP, "the console cannot put LiMo on a child’s phone"),
        (NOTICES, "the third party notices are missing"),
    ] {
        if !run.m.exists(&places.install_dir.join(part)) {
            run.say(&format!("Not in the program folder: {part}. Without it {without}. The installer puts it there."));
        }
    }
    Ok(())
}

/// The data folder: made with its protection, or the one that is there
/// kept, its protection set back when it was changed.
fn data_folder<M: Machine>(run: &mut Run<'_, M>, places: &Places, data: &DataFolder) -> Result<(), String> {
    let folder = &places.data_dir;
    match data {
        DataFolder::Absent => {
            run.m.make_protected_folder(folder).map_err(|e| format!("making the data folder {}: {e}", folder.display()))?;
            run.say(&format!("Made the data folder {}, for SYSTEM and Administrators only.", folder.display()));
        }
        DataFolder::Kept(FolderVerdict::AccessChanged) => {
            run.m.protect_folder(folder).map_err(|e| format!("setting the data folder’s protection back: {e}"))?;
            run.say(&format!(
                "The data folder {}’s access list had been changed; it is set back to SYSTEM and Administrators only.",
                folder.display()
            ));
        }
        DataFolder::Kept(_) => run.say(&format!("Kept the data folder {}, and the family’s file in it.", folder.display())),
    }
    let seen = run.m.folder_seen(folder);
    if let Some(seen) = &seen {
        run.say(&format!("  Its owner and access list, as Windows has them: O:{} {}", seen.owner, seen.dacl));
    }
    match judge_folder(seen.as_ref()) {
        FolderVerdict::Limos => Ok(()),
        FolderVerdict::AccessChanged => Err("the data folder’s access list is not LiMo’s after it was set".to_string()),
        FolderVerdict::NotLimos(why) => Err(format!("the data folder is not LiMo’s after it was made: {why}")),
    }
}

/// The settings file's test cap, and the record of what is installed.
fn settings_and_record<M: Machine>(run: &mut Run<'_, M>, places: &Places, dev: bool, version: &str) -> Result<(), String> {
    let file = places.settings_file();
    let mut settings = run.m.read_text(&file).map(|text| read_settings(&text)).unwrap_or_default();
    let was_on = settings.test_cap;
    settings.test_cap = dev;
    run.m.write_text(&file, &settings_text(&settings)).map_err(|e| format!("writing the settings: {e}"))?;
    if dev {
        run.say("TEST CAP ON: this is a test install. Any single lock is lifted after 120 seconds, and the log says so each time.");
    } else if was_on {
        run.say("The test cap of the earlier test install is now off.");
    }
    let record = serde_json::to_string_pretty(&InstallRecord { version: version.to_string() }).unwrap_or_default();
    run.m.write_text(&places.data_dir.join(RECORD_FILE), &format!("{record}\n")).map_err(|e| format!("writing the record: {e}"))?;
    Ok(())
}

/// LiMo's rules in Windows' firewall, each made again so that it is never
/// there twice. How many could not be added.
fn add_firewall_rules<M: Machine>(run: &mut Run<'_, M>, exe: &Path) -> u32 {
    let mut not_done = 0;
    for rule in firewall_rules(exe) {
        let added = remove_rules_named(run.m, &rule.name).and_then(|_| run.m.add_firewall_rule(&rule));
        match added {
            Ok(()) => run.say(&format!("Windows’ firewall lets in, by the rule \"{}\": {}.", rule.name, rule.summary())),
            Err(problem) => {
                run.say(&format!(
                    "NOT DONE: the firewall rule \"{}\" could not be added: {problem}. Phones on the home network cannot reach \
                     this PC until it is (the cable still works). Run the installer again.",
                    rule.name
                ));
                not_done += 1;
            }
        }
    }
    not_done
}

/// Removes every rule of this name; how many were removed.
fn remove_rules_named(m: &mut impl Machine, name: &str) -> Result<usize, String> {
    let mut removed = 0;
    // A name is never on more than a few rules; the bound keeps a firewall
    // that will not let go from holding the run.
    while m.firewall_has_rule(name)? && removed < 16 {
        m.remove_firewall_rule(name)?;
        removed += 1;
    }
    Ok(removed)
}

/// The console's shortcut, in the Start menu of every account: a child who
/// opens it is refused by the door itself. How many steps were not done.
fn make_shortcut<M: Machine>(run: &mut Run<'_, M>, places: &Places) -> u32 {
    let console = places.install_dir.join(CONSOLE_EXE);
    if !run.m.exists(&console) {
        run.say("No shortcut was made: there is no console in the program folder.");
        return 0;
    }
    let at = run.m.shortcut_folder().join(SHORTCUT_FILE);
    match run.m.make_shortcut(&at, &console, SHORTCUT_DESCRIPTION) {
        Ok(()) => {
            run.say(&format!(
                "Made the shortcut {} for every account. LiMo’s settings refuse a child’s account by themselves.",
                at.display()
            ));
            0
        }
        Err(problem) => {
            run.say(&format!("NOT DONE: the shortcut {} could not be made: {problem}.", at.display()));
            1
        }
    }
}

/// `limo stop-for-upgrade`: before an installer copies the new files.
pub fn stop_for_upgrade(m: &mut impl Machine) -> i32 {
    let mut run = Run::new(m);
    let code = stop_steps(&mut run);
    run.finish(code)
}

fn stop_steps<M: Machine>(run: &mut Run<'_, M>) -> i32 {
    run.say(&format!("LiMo for Windows {}: stop for an upgrade.", crate::windows_version()));
    if !run.m.elevated() {
        run.say(&format!("Stopping LiMo for an upgrade {NEEDS_ADMINISTRATOR} Nothing was stopped."));
        return 2;
    }
    let places = run.m.places();
    let me = run.m.this_exe();
    if inside(&me, &places.install_dir) {
        run.say(&format!(
            "stop-for-upgrade runs from a copy outside {}: this one is inside it, and would hold its own file. Nothing was stopped.",
            places.install_dir.display()
        ));
        return 2;
    }
    let service = match run.m.service() {
        Ok(service) => service,
        Err(problem) => {
            run.say(&format!("The service control manager could not be asked about the service: {problem}. Nothing was stopped."));
            return 1;
        }
    };
    if service.is_none() && run.m.running_from(&places.install_dir).is_empty() {
        run.say("LiMo is not installed on this PC, so there is nothing to stop.");
        return 0;
    }
    match stop_limo(run, &places, service.as_ref()) {
        Ok(()) => {
            run.say(&format!(
                "Nothing of LiMo runs from {} now, so its files can be replaced. limo install starts the service again.",
                places.install_dir.display()
            ));
            0
        }
        Err(problem) => {
            run.say(&format!("Stopping for the upgrade failed: {problem}."));
            1
        }
    }
}

/// `limo uninstall [--keep-family-file | --delete-family-file]`: when a
/// family exists, the guardian password first; then the service, the
/// programs that run, the firewall rules, the shortcut, the program folder
/// (unless an installer removes it), and the data folder kept or deleted.
pub fn uninstall(m: &mut impl Machine, choice: Option<FamilyFile>) -> i32 {
    let mut run = Run::new(m);
    let code = uninstall_steps(&mut run, choice);
    run.finish(code)
}

fn uninstall_steps<M: Machine>(run: &mut Run<'_, M>, choice: Option<FamilyFile>) -> i32 {
    run.say(&format!("LiMo for Windows {}: uninstall.", crate::windows_version()));
    if !run.m.elevated() {
        run.say(&format!(
            "Removing LiMo {NEEDS_ADMINISTRATOR} It can also be removed from Windows’ list of apps. Nothing was removed."
        ));
        return 2;
    }
    let places = run.m.places();
    let me = run.m.this_exe();
    let from_installed = inside(&me, &places.install_dir);
    let service = match run.m.service() {
        Ok(service) => service,
        Err(problem) => {
            run.say(&format!("The service control manager could not be asked about the service: {problem}. Nothing was removed."));
            return 1;
        }
    };
    let data_present = run.m.exists(&places.data_dir);
    let data_verdict = data_present.then(|| {
        let seen = run.m.folder_seen(&places.data_dir);
        judge_folder(seen.as_ref())
    });

    // The guardian's word, before anything is removed.
    let running = service.as_ref().is_some_and(|s| s.state != ServiceRun::Stopped);
    let gate = if running {
        through_the_door(run)
    } else if matches!(data_verdict, Some(FolderVerdict::Limos | FolderVerdict::AccessChanged))
        && run.m.exists(&places.family_file())
    {
        in_the_file(run)
    } else {
        run.say("There is no family’s file, so no password is needed.");
        0
    };
    run.m.door_close();
    if gate != 0 {
        return gate;
    }

    // Whether the family's file stays, asked before anything goes.
    let keep = if data_present { Some(family_file_choice(run, choice)) } else { None };

    let mut not_done = 0;
    match &service {
        Some(seen) => {
            if seen.state != ServiceRun::Stopped {
                match run.m.stop_service() {
                    Ok(()) => run.say(&format!("Stopped the service {SERVICE_NAME}.")),
                    Err(problem) => run.say(&format!("The service could not be stopped: {problem}; it is deleted all the same.")),
                }
            }
            match run.m.delete_service() {
                Ok(()) => run.say(&format!("Removed the service {SERVICE_NAME}.")),
                Err(problem) => {
                    run.say(&format!("Uninstall failed: the service could not be removed: {problem}. Nothing else was removed."));
                    return 1;
                }
            }
        }
        None => run.say(&format!("The service {SERVICE_NAME} was not installed.")),
    }
    for _ in 0..4 {
        if run.m.running_from(&places.install_dir).is_empty() {
            break;
        }
        run.m.pause(500);
    }
    for process in run.m.running_from(&places.install_dir) {
        match run.m.end_process(&process) {
            Ok(()) => run.say(&format!("Ended {} (pid {}) in session {}.", process.name, process.pid, process.session_id)),
            Err(problem) => run.say(&format!("{} (pid {}) could not be ended: {problem}.", process.name, process.pid)),
        }
    }
    for rule in firewall_rules(&places.installed_exe()) {
        match remove_rules_named(run.m, &rule.name) {
            Ok(0) => run.say(&format!("The firewall rule \"{}\" was not there.", rule.name)),
            Ok(_) => run.say(&format!("Removed the firewall rule \"{}\".", rule.name)),
            Err(problem) => {
                run.say(&format!("NOT DONE: the firewall rule \"{}\" could not be removed: {problem}.", rule.name));
                not_done += 1;
            }
        }
    }
    let shortcut = run.m.shortcut_folder().join(SHORTCUT_FILE);
    if run.m.exists(&shortcut) {
        match run.m.remove_file(&shortcut) {
            Ok(()) => run.say(&format!("Removed the shortcut {}.", shortcut.display())),
            Err(problem) => {
                run.say(&format!("NOT DONE: the shortcut {} could not be removed: {problem}.", shortcut.display()));
                not_done += 1;
            }
        }
    }
    if from_installed {
        run.say(&format!("This program runs from {}, so the installer removes that folder after it.", places.install_dir.display()));
    } else if run.m.exists(&places.install_dir) {
        match run.m.remove_folder(&places.install_dir) {
            Ok(()) => run.say(&format!("Removed {}.", places.install_dir.display())),
            Err(problem) => {
                run.say(&format!("NOT DONE: {} could not be removed: {problem}.", places.install_dir.display()));
                not_done += 1;
            }
        }
    }
    match keep {
        Some(FamilyFile::Keep) => run.say(&format!(
            "Kept {}: the family’s file, its log and settings. Installing LiMo again uses them, with the same guardian password.",
            places.data_dir.display()
        )),
        Some(FamilyFile::Delete) => match run.m.remove_folder(&places.data_dir) {
            Ok(()) => run.say(&format!("Deleted {}, with the family’s file in it.", places.data_dir.display())),
            Err(problem) => {
                run.say(&format!("NOT DONE: {} could not be deleted: {problem}.", places.data_dir.display()));
                not_done += 1;
            }
        },
        None => {}
    }
    if not_done == 0 {
        run.say("LiMo is removed.");
        0
    } else {
        run.say(&format!("LiMo is removed, but {not_done} step(s) were not done: see above."));
        1
    }
}

/// What the uninstall says when it cannot have the password.
const NO_PASSWORD: &str = "Nothing was removed. A guardian can sign in to LiMo’s settings and choose to remove LiMo, \
     then remove it within ten minutes with no password; an administrator whose family lost the password can set a new \
     one with limo reset-password first.";

/// The password through the door of LiMo's settings: 0 when it may go on.
fn through_the_door<M: Machine>(run: &mut Run<'_, M>) -> i32 {
    // A service that has just started opens its door within a moment.
    let mut found = run.m.door_open();
    for _ in 0..DOOR_TRIES {
        if !matches!(found, DoorOpen::Absent(_)) {
            break;
        }
        run.m.pause(1_000);
        found = run.m.door_open();
    }
    match found {
        DoorOpen::Open { .. } => {}
        DoorOpen::Refused(message) => {
            run.say(&message);
            run.say("LiMo is removed from a guardian’s account. Nothing was removed.");
            return 2;
        }
        DoorOpen::NotTheService(why) => {
            run.say(&format!("{why} Nothing was sent, and nothing was removed."));
            return 2;
        }
        DoorOpen::Absent(problem) => {
            run.say(&format!(
                "LiMo’s settings did not answer ({problem}), so the guardian password cannot be asked. Nothing was removed. \
                 Try again in a moment. If the service is stuck, stop it (sc.exe stop {SERVICE_NAME}) and run uninstall \
                 again: the password is then checked in the family’s file itself."
            ));
            return 1;
        }
    }
    let state = match run.m.door_ask(Request::FamilyState) {
        Ok(Reply::Done { value }) => value,
        Ok(Reply::Problem { problem }) => {
            run.say(&format!("{} Nothing was removed.", problem.message));
            return 1;
        }
        Err(problem) => {
            run.say(&format!("LiMo’s settings stopped answering: {problem}. Nothing was removed."));
            return 1;
        }
    };
    if state["has_family"] != true {
        run.say("The family’s file has no family yet, so no password is needed.");
        return 0;
    }
    if state["removal_allowed_until"].as_i64().is_some() {
        run.say("A guardian allowed LiMo’s removal in its settings a moment ago, so no password is needed.");
        return 0;
    }
    for _ in 0..PASSWORD_TRIES {
        let Some(password) = run.m.ask_secret("Guardian password, to remove LiMo (not shown): ").filter(|p| !p.expose().is_empty())
        else {
            run.say(&format!("No password was typed. {NO_PASSWORD}"));
            return 2;
        };
        // Signing in is the password's check at the door, with the
        // family's waits; the session ends when the line closes.
        match run.m.door_ask(Request::SignIn { password }) {
            Ok(Reply::Done { .. }) => {
                run.say("The guardian password is right.");
                return 0;
            }
            Ok(Reply::Problem { problem }) if problem.kind == ProblemKind::WrongPassword && problem.wait_until.is_none() => {
                run.say(&problem.message);
            }
            Ok(Reply::Problem { problem }) if matches!(problem.kind, ProblemKind::WrongPassword | ProblemKind::MustWait) => {
                run.say(&format!("{} Nothing was removed.", problem.message));
                return 2;
            }
            Ok(Reply::Problem { problem }) => {
                run.say(&format!("{} Nothing was removed.", problem.message));
                return 1;
            }
            Err(problem) => {
                run.say(&format!("LiMo’s settings stopped answering: {problem}. Nothing was removed."));
                return 1;
            }
        }
    }
    run.say(&format!("The password was not right {PASSWORD_TRIES} times. {NO_PASSWORD}"));
    2
}

/// The password checked in the family's file itself, when the service
/// does not run: the same waits. 0 when it may go on.
fn in_the_file<M: Machine>(run: &mut Run<'_, M>) -> i32 {
    match run.m.file_has_family() {
        Ok(false) => {
            run.say("The family’s file has no family yet, so no password is needed.");
            return 0;
        }
        Ok(true) => run.say("The service does not run, so the guardian password is checked in the family’s file itself."),
        Err(problem) => {
            run.say(&format!(
                "The family’s file could not be read, so the guardian password cannot be checked: {problem}. Nothing was removed."
            ));
            return 1;
        }
    }
    for _ in 0..PASSWORD_TRIES {
        let Some(password) = run.m.ask_secret("Guardian password, to remove LiMo (not shown): ").filter(|p| !p.expose().is_empty())
        else {
            run.say(&format!("No password was typed. {NO_PASSWORD}"));
            return 2;
        };
        let now = run.m.wall_secs();
        match run.m.file_check_password(&password, now) {
            Ok(PasswordOutcome::Right) => {
                run.say("The guardian password is right.");
                return 0;
            }
            Ok(outcome) => {
                let problem = crate::service::door::password_problem(outcome);
                if problem.wait_until.is_some() {
                    run.say(&format!("{} Nothing was removed.", problem.message));
                    return 2;
                }
                run.say(&problem.message);
            }
            Err(problem) => {
                run.say(&format!("The family’s file could not check the password: {problem}. Nothing was removed."));
                return 1;
            }
        }
    }
    run.say(&format!("The password was not right {PASSWORD_TRIES} times. {NO_PASSWORD}"));
    2
}

/// Whether the family's file is kept: as the command line said, or asked
/// plainly. Kept when nothing can be asked, so that nothing is lost.
fn family_file_choice<M: Machine>(run: &mut Run<'_, M>, choice: Option<FamilyFile>) -> FamilyFile {
    if let Some(choice) = choice {
        let words = match choice {
            FamilyFile::Keep => "kept",
            FamilyFile::Delete => "deleted",
        };
        run.say(&format!("The family’s file is {words}, as the command line asked."));
        return choice;
    }
    for _ in 0..3 {
        let answer = run.m.ask_line(
            "Keep the family’s file? Kept, LiMo installed again has the children, their rules, their devices and the \
             guardian password as they are now. Deleted, they are gone for good. Type keep or delete: ",
        );
        let Some(answer) = answer else {
            run.say("No answer could be read, so the family’s file is kept.");
            return FamilyFile::Keep;
        };
        match answer.trim().to_lowercase().as_str() {
            "keep" | "k" => {
                run.say("The family’s file is kept.");
                return FamilyFile::Keep;
            }
            "delete" | "d" => {
                run.say("The family’s file is deleted.");
                return FamilyFile::Delete;
            }
            _ => run.m.say("Type keep or delete."),
        }
    }
    run.say("No answer was keep or delete, so the family’s file is kept.");
    FamilyFile::Keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_limos_only_when_system_or_administrators_made_it_with_limos_protection() {
        let seen = |owner: &str, dacl: &str| FolderSeen { owner: owner.into(), dacl: dacl.into() };
        assert_eq!(judge_folder(Some(&seen("BA", DATA_FOLDER_SDDL))), FolderVerdict::Limos);
        assert_eq!(judge_folder(Some(&seen("SY", "D:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)"))), FolderVerdict::Limos);
        assert_eq!(judge_folder(Some(&seen("S-1-5-32-544", DATA_FOLDER_SDDL))), FolderVerdict::Limos);
        assert_eq!(judge_folder(Some(&seen("BA", "D:(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"))), FolderVerdict::AccessChanged, "inherits");
        assert_eq!(
            judge_folder(Some(&seen("BA", "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)"))),
            FolderVerdict::AccessChanged
        );
        assert!(matches!(judge_folder(Some(&seen("S-1-5-21-1001", DATA_FOLDER_SDDL))), FolderVerdict::NotLimos(why) if why.contains("S-1-5-21-1001")));
        assert!(matches!(judge_folder(None), FolderVerdict::NotLimos(_)));
        assert_eq!(judge_folder(Some(&seen("BA", "not an access list"))), FolderVerdict::AccessChanged);
    }

    #[test]
    fn versions_are_three_whole_numbers() {
        assert_eq!(version_numbers("0.1.0"), Some((0, 1, 0)));
        assert_eq!(version_numbers(" 1.12.3 "), Some((1, 12, 3)));
        assert!(version_numbers("1.2").is_none() && version_numbers("1.2.3.4").is_none() && version_numbers("1.x.3").is_none());
        assert!(version_numbers("0.10.0") > version_numbers("0.9.9"));
    }

    #[test]
    fn the_two_firewall_rules_are_for_limo_exe_only_from_the_local_subnet() {
        let exe = Path::new(r"C:\Program Files\LiMo\limo.exe");
        let rules = firewall_rules(exe);
        assert_eq!(rules.len(), 2);
        assert_eq!((rules[0].protocol, rules[0].port), (Protocol::Tcp, 47810));
        assert_eq!((rules[1].protocol, rules[1].port), (Protocol::Udp, 5353));
        for rule in &rules {
            assert_eq!(rule.program, exe);
            assert!(rule.name.starts_with("LiMo, ") && rule.description.contains("limo.exe only"));
            assert!(rule.summary().contains("from the local subnet only") && rule.summary().contains(r"C:\Program Files\LiMo\limo.exe"));
        }
        assert_ne!(rules[0].name, rules[1].name);
        assert_eq!(RULE_REMOTE_ADDRESSES, "LocalSubnet");
        const { assert!(RULE_PROFILES.private && RULE_PROFILES.public && !RULE_PROFILES.domain) };
    }
}
