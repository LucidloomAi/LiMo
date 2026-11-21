//! The installer's work in `limo.exe`, against a fake PC: `limo install`
//! (new, over itself, over a test install, over an older or newer LiMo,
//! with a data folder kept or made by someone else), `limo
//! stop-for-upgrade`, and `limo uninstall` asking for the guardian password
//! through the console's door before it removes anything. The door is the
//! real service's, on the stories' fake PC, with a real family file, so the
//! password, its waits and the refusal of a child's account are the real
//! ones. Nothing here touches the real PC: no service, no firewall, no
//! folder outside the test's own temporary one.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use limo_hub::{AccountLink, PasswordOutcome};
use limo_windows::console::{Reply, Request, ToConsole};
use limo_windows::installer::{
    CONSOLE_EXE, DoorOpen, FamilyFile, FirewallRule, FolderSeen, Machine, NOTICES, PHONE_APP, Protocol, RECORD_FILE, SETUP_LOG,
    SETUP_LOG_AFTER_DELETE, SHORTCUT_FILE, ServiceRun, ServiceSeen, install, stop_for_upgrade, uninstall,
};
use limo_windows::os::{DoorId, ProcessSeen};
use limo_windows::protocol::Secret;
use limo_windows::settings::{DATA_FOLDER_SDDL, Places, read_settings};

use crate::common::*;

const GUARDIAN: &str = "S-1-5-21-1000";
const ASHA: &str = "S-1-5-21-1001";
const PROGRAMS: &str = r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs";
const TEMP: &str = r"C:\Temp";
const BUILD: &str = r"C:\build\limo\target\release";
/// This process, as the fake PC sees it.
const ME: u32 = 4242;
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The fake PC of an install, with the stories' PC behind its door.
struct Fake {
    pc: Pc,
    elevated: bool,
    exe: PathBuf,
    files: BTreeMap<PathBuf, String>,
    folders: BTreeSet<PathBuf>,
    protection: BTreeMap<PathBuf, FolderSeen>,
    service: Option<ServiceSeen>,
    rules: Vec<FirewallRule>,
    shortcuts: BTreeMap<PathBuf, PathBuf>,
    processes: Vec<ProcessSeen>,
    ended: Vec<u32>,
    said: Vec<String>,
    secrets: VecDeque<Option<String>>,
    secrets_asked: usize,
    lines: VecDeque<Option<String>>,
    lines_asked: usize,
    door: Option<DoorId>,
    door_account: String,
    /// The service runs, but its door does not answer.
    door_shut: bool,
    doors_tried: u32,
    fail: BTreeSet<&'static str>,
    /// Every change, in order.
    changes: Vec<String>,
}

fn places() -> Places {
    Places::under(Path::new(r"C:\Program Files"), Path::new(r"C:\ProgramData"))
}

fn under(path: &Path, folder: &Path) -> bool {
    path.starts_with(folder)
}

impl Fake {
    /// A PC where the installer has just copied LiMo's files, and this
    /// process is the installed `limo.exe`. The stories' PC behind the door
    /// has a family when `family`.
    fn new(family: bool) -> Fake {
        let places = places();
        let pc = if family { Pc::new(at(16, 0)) } else { Pc::empty(at(16, 0)) };
        let mut fake = Fake {
            pc,
            elevated: true,
            exe: places.installed_exe(),
            files: BTreeMap::new(),
            folders: BTreeSet::new(),
            protection: BTreeMap::new(),
            service: None,
            rules: Vec::new(),
            shortcuts: BTreeMap::new(),
            processes: Vec::new(),
            ended: Vec::new(),
            said: Vec::new(),
            secrets: VecDeque::new(),
            secrets_asked: 0,
            lines: VecDeque::new(),
            lines_asked: 0,
            door: None,
            door_account: GUARDIAN.to_string(),
            door_shut: false,
            doors_tried: 0,
            fail: BTreeSet::new(),
            changes: Vec::new(),
        };
        fake.folders.insert(places.install_dir.clone());
        fake.folders.insert(places.install_dir.join(NOTICES));
        fake.folders.insert(PathBuf::from(PROGRAMS));
        fake.folders.insert(PathBuf::from(TEMP));
        for file in ["limo.exe", CONSOLE_EXE, PHONE_APP] {
            fake.files.insert(places.install_dir.join(file), format!("{file} {VERSION}"));
        }
        fake
    }

    /// A PC with this LiMo installed and running, a helper in a child's
    /// session and the console open in the guardian's.
    fn installed(family: bool) -> Fake {
        let mut fake = Fake::new(family);
        assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
        if family {
            fake.files.insert(places().family_file(), "the family".into());
        }
        fake.run_limo();
        fake.forget();
        fake
    }

    /// LiMo's programs running from the program folder, and one that is not
    /// LiMo's.
    fn run_limo(&mut self) {
        let dir = places().install_dir;
        self.processes = vec![
            ProcessSeen { pid: 7001, session_id: 2, started: 1, name: "limo.exe".into(), path: dir.join("limo.exe").to_string_lossy().into() },
            ProcessSeen { pid: 7002, session_id: 1, started: 2, name: CONSOLE_EXE.into(), path: dir.join(CONSOLE_EXE).to_string_lossy().into() },
            ProcessSeen { pid: 7003, session_id: 1, started: 3, name: "notepad.exe".into(), path: r"C:\Windows\notepad.exe".into() },
        ];
    }

    /// Forgets what was said and changed so far.
    fn forget(&mut self) {
        self.said.clear();
        self.changes.clear();
        self.ended.clear();
    }

    fn change(&mut self, what: impl Into<String>) {
        self.changes.push(what.into());
    }

    fn failing(&self, step: &str) -> Result<(), String> {
        if self.fail.contains(step) { Err(format!("{step} was made to fail")) } else { Ok(()) }
    }

    fn said(&self, text: &str) -> bool {
        self.said.iter().any(|line| line.contains(text))
    }

    fn setup_log(&self) -> String {
        self.files.get(&places().data_dir.join(SETUP_LOG)).cloned().unwrap_or_default()
    }

    fn settings(&self) -> limo_windows::settings::Settings {
        read_settings(self.files.get(&places().settings_file()).map_or("", String::as_str))
    }

    fn running(&self) -> bool {
        self.service.as_ref().is_some_and(|s| s.state == ServiceRun::Running)
    }

    /// Everything said and logged: no password, ever.
    fn assert_no_secret(&self) {
        for text in self.said.iter().chain(self.files.values()) {
            for secret in [PASSWORD, "not the password", PIN] {
                assert!(!text.contains(secret), "a secret was said or written: {text}");
            }
        }
    }
}

impl Machine for Fake {
    fn elevated(&mut self) -> bool {
        self.elevated
    }
    fn this_exe(&mut self) -> PathBuf {
        self.exe.clone()
    }
    fn places(&mut self) -> Places {
        places()
    }
    fn shortcut_folder(&mut self) -> PathBuf {
        PathBuf::from(PROGRAMS)
    }
    fn temp_folder(&mut self) -> PathBuf {
        PathBuf::from(TEMP)
    }
    fn timestamp(&mut self) -> String {
        "2026-11-21 10:00:00".into()
    }
    fn wall_secs(&mut self) -> i64 {
        self.pc.now()
    }
    fn exists(&mut self, path: &Path) -> bool {
        self.files.contains_key(path) || self.folders.contains(path)
    }
    fn read_text(&mut self, path: &Path) -> Option<String> {
        self.files.get(path).cloned()
    }
    fn write_text(&mut self, path: &Path, text: &str) -> Result<(), String> {
        self.failing("write")?;
        self.change(format!("write {}", path.display()));
        self.files.insert(path.to_path_buf(), text.to_string());
        Ok(())
    }
    fn append_text(&mut self, path: &Path, text: &str) -> Result<(), String> {
        if !path.parent().is_some_and(|folder| self.folders.contains(folder)) {
            return Err("no such folder".into());
        }
        self.files.entry(path.to_path_buf()).or_default().push_str(text);
        Ok(())
    }
    fn copy_file(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        self.failing("copy")?;
        let text = self.files.get(from).cloned().ok_or("no such file")?;
        self.change(format!("copy {} to {}", from.display(), to.display()));
        self.files.insert(to.to_path_buf(), text);
        Ok(())
    }
    fn make_folder(&mut self, path: &Path) -> Result<(), String> {
        self.change(format!("make folder {}", path.display()));
        self.folders.insert(path.to_path_buf());
        Ok(())
    }
    fn remove_file(&mut self, path: &Path) -> Result<(), String> {
        self.change(format!("remove {}", path.display()));
        self.files.remove(path);
        self.shortcuts.remove(path);
        Ok(())
    }
    fn remove_folder(&mut self, path: &Path) -> Result<(), String> {
        self.change(format!("remove folder {}", path.display()));
        self.files.retain(|file, _| !under(file, path));
        self.folders.retain(|folder| !under(folder, path));
        self.protection.remove(path);
        Ok(())
    }
    fn make_protected_folder(&mut self, path: &Path) -> Result<(), String> {
        self.failing("data folder")?;
        self.change(format!("make the data folder {}", path.display()));
        self.folders.insert(path.to_path_buf());
        self.protection.insert(path.to_path_buf(), FolderSeen { owner: "BA".into(), dacl: DATA_FOLDER_SDDL.into() });
        Ok(())
    }
    fn protect_folder(&mut self, path: &Path) -> Result<(), String> {
        self.change(format!("protect {}", path.display()));
        if let Some(seen) = self.protection.get_mut(path) {
            seen.dacl = DATA_FOLDER_SDDL.into();
        }
        Ok(())
    }
    fn folder_seen(&mut self, path: &Path) -> Option<FolderSeen> {
        self.protection.get(path).cloned()
    }
    fn service(&mut self) -> Result<Option<ServiceSeen>, String> {
        self.failing("ask the service")?;
        Ok(self.service.clone())
    }
    fn create_service(&mut self, exe: &Path) -> Result<(), String> {
        self.failing("create the service")?;
        self.change("create the service");
        self.service = Some(ServiceSeen { state: ServiceRun::Stopped, exe: Some(exe.to_path_buf()) });
        Ok(())
    }
    fn configure_service(&mut self, exe: &Path) -> Result<(), String> {
        self.failing("configure the service")?;
        self.change("configure the service");
        if let Some(service) = self.service.as_mut() {
            service.exe = Some(exe.to_path_buf());
        }
        Ok(())
    }
    fn start_service(&mut self) -> Result<(), String> {
        self.failing("start the service")?;
        self.change("start the service");
        if let Some(service) = self.service.as_mut() {
            service.state = ServiceRun::Running;
        }
        Ok(())
    }
    fn stop_service(&mut self) -> Result<(), String> {
        self.failing("stop the service")?;
        self.change("stop the service");
        if let Some(service) = self.service.as_mut() {
            service.state = ServiceRun::Stopped;
        }
        Ok(())
    }
    fn delete_service(&mut self) -> Result<(), String> {
        self.change("delete the service");
        self.service = None;
        Ok(())
    }
    fn running_from(&mut self, folder: &Path) -> Vec<ProcessSeen> {
        self.processes.iter().filter(|p| p.pid != ME && under(Path::new(&p.path), folder)).cloned().collect()
    }
    fn end_process(&mut self, process: &ProcessSeen) -> Result<(), String> {
        self.change(format!("end {}", process.pid));
        self.ended.push(process.pid);
        self.processes.retain(|p| p.pid != process.pid);
        Ok(())
    }
    fn pause(&mut self, _ms: u64) {}
    fn firewall_has_rule(&mut self, name: &str) -> Result<bool, String> {
        self.failing("firewall")?;
        Ok(self.rules.iter().any(|rule| rule.name == name))
    }
    fn add_firewall_rule(&mut self, rule: &FirewallRule) -> Result<(), String> {
        self.change(format!("add the rule {}", rule.name));
        self.rules.push(rule.clone());
        Ok(())
    }
    fn remove_firewall_rule(&mut self, name: &str) -> Result<(), String> {
        self.change(format!("remove the rule {name}"));
        if let Some(at) = self.rules.iter().position(|rule| rule.name == name) {
            self.rules.remove(at);
        }
        Ok(())
    }
    fn make_shortcut(&mut self, at: &Path, target: &Path, _description: &str) -> Result<(), String> {
        self.change(format!("make the shortcut {}", at.display()));
        self.shortcuts.insert(at.to_path_buf(), target.to_path_buf());
        self.files.insert(at.to_path_buf(), "a shortcut".into());
        Ok(())
    }
    fn door_open(&mut self) -> DoorOpen {
        self.doors_tried += 1;
        if self.door_shut || !self.running() {
            return DoorOpen::Absent("the pipe is not there".into());
        }
        let account = self.door_account.clone();
        let id = self.pc.console(&account, 1);
        match self.pc.hello(id) {
            Some(ToConsole::Hello { version, .. }) => {
                self.door = Some(id);
                DoorOpen::Open { version }
            }
            Some(ToConsole::Refused { message }) => DoorOpen::Refused(message),
            other => DoorOpen::Absent(format!("{other:?}")),
        }
    }
    fn door_ask(&mut self, request: Request) -> Result<Reply, String> {
        let id = self.door.ok_or("the door is not open")?;
        // A second after the last, as the real line waits for a secret.
        self.pc.run(1);
        self.pc.ask(id, request).ok_or_else(|| "no answer".to_string())
    }
    fn door_close(&mut self) {
        if let Some(id) = self.door.take() {
            self.pc.os().disconnect(id);
            self.pc.step();
        }
    }
    fn file_has_family(&mut self) -> Result<bool, String> {
        self.pc.hub().has_family().map_err(|e| e.to_string())
    }
    fn file_check_password(&mut self, password: &Secret, now: i64) -> Result<PasswordOutcome, String> {
        self.pc.hub().check_password(password.expose(), now).map_err(|e| e.to_string())
    }
    fn ask_secret(&mut self, _prompt: &str) -> Option<Secret> {
        self.secrets_asked += 1;
        self.secrets.pop_front().flatten().map(Secret::new)
    }
    fn ask_line(&mut self, _prompt: &str) -> Option<String> {
        self.lines_asked += 1;
        self.lines.pop_front().flatten()
    }
    fn say(&mut self, text: &str) {
        self.said.push(text.to_string());
    }
}

fn typed(text: &str) -> Option<String> {
    Some(text.to_string())
}

#[test]
fn without_elevation_install_upgrade_and_uninstall_are_refused_and_change_nothing() {
    let mut fake = Fake::new(true);
    fake.elevated = false;
    assert_eq!(install(&mut fake, false), 2);
    assert!(fake.said("Installing LiMo needs an administrator") && fake.said("Nothing was changed."), "{:#?}", fake.said);
    assert_eq!(stop_for_upgrade(&mut fake), 2);
    assert!(fake.said("Stopping LiMo for an upgrade needs an administrator"));
    assert_eq!(uninstall(&mut fake, None), 2);
    assert!(fake.said("Removing LiMo needs an administrator") && fake.said("Nothing was removed."));
    assert!(fake.changes.is_empty(), "{:?}", fake.changes);
    assert_eq!(fake.secrets_asked, 0);
    // Each run is still written down, where this account may write.
    assert!(fake.files.get(&PathBuf::from(TEMP).join(SETUP_LOG_AFTER_DELETE)).is_some_and(|log| log.contains("needs an administrator")));
}

#[test]
fn a_new_install_from_the_installer_makes_every_part_in_order_and_logs_each_step() {
    let mut fake = Fake::new(false);
    assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
    let places = places();
    // The data folder, with its protection from the first moment.
    assert_eq!(fake.protection[&places.data_dir], FolderSeen { owner: "BA".into(), dacl: DATA_FOLDER_SDDL.into() });
    assert!(!fake.settings().test_cap);
    assert!(fake.files[&places.data_dir.join(RECORD_FILE)].contains(&format!("\"version\": \"{VERSION}\"")));
    // The service, for the installed program, running.
    assert_eq!(fake.service, Some(ServiceSeen { state: ServiceRun::Running, exe: Some(places.installed_exe()) }));
    // The two firewall rules, for limo.exe only.
    assert_eq!(fake.rules.len(), 2);
    assert_eq!((fake.rules[0].protocol, fake.rules[0].port), (Protocol::Tcp, 47810));
    assert_eq!((fake.rules[1].protocol, fake.rules[1].port), (Protocol::Udp, 5353));
    assert!(fake.rules.iter().all(|rule| rule.program == places.installed_exe()));
    // The shortcut for every account, to the console.
    assert_eq!(fake.shortcuts[&PathBuf::from(PROGRAMS).join(SHORTCUT_FILE)], places.install_dir.join(CONSOLE_EXE));
    // Nothing copied: the installer put the files there.
    assert!(fake.changes.iter().all(|c| !c.starts_with("copy")), "{:?}", fake.changes);
    assert!(fake.said("put there by the installer") && fake.said(&format!("A new install of LiMo {VERSION}.")));
    assert!(!fake.said("Not in the program folder"));
    // In order: the folder before the service, the service started last.
    let at = |what: &str| fake.changes.iter().position(|c| c.starts_with(what)).unwrap_or_else(|| panic!("{what}: {:?}", fake.changes));
    assert!(at("make the data folder") < at("create the service"));
    assert!(at("create the service") < at("add the rule"));
    assert_eq!(fake.changes.last().map(String::as_str), Some("start the service"));
    // Every step is in the setup log, in the data folder, with its time.
    let log = fake.setup_log();
    for step in ["Made the data folder", "Created the service LiMo", "Windows’ firewall lets in", "Made the shortcut", "Started the service", "is installed."] {
        assert!(log.contains(step), "{step} not in the log: {log}");
    }
    assert!(log.lines().all(|line| line.is_empty() || line.starts_with("2026-11-21 10:00:00 ")), "{log}");
    assert!(log.contains("The run ended with exit code 0."));
    assert!(fake.said(&format!("The steps of this run are written in {}", places.data_dir.join(SETUP_LOG).display())));
}

#[test]
fn a_test_install_from_a_build_folder_copies_the_program_and_switches_the_test_cap_on() {
    let mut fake = Fake::new(false);
    let places = places();
    fake.files.clear();
    fake.folders.retain(|folder| !folder.starts_with(&places.install_dir));
    fake.exe = PathBuf::from(BUILD).join("limo.exe");
    fake.files.insert(fake.exe.clone(), "a build".into());
    assert_eq!(install(&mut fake, true), 0, "{:#?}", fake.said);
    assert_eq!(fake.files[&places.installed_exe()], "a build");
    assert!(fake.said(&format!("Copied the program to {}", places.installed_exe().display())));
    assert!(fake.settings().test_cap && fake.said("TEST CAP ON"));
    // What only the installer brings is said, and the shortcut not made.
    for part in [CONSOLE_EXE, PHONE_APP, NOTICES] {
        assert!(fake.said(&format!("Not in the program folder: {part}.")), "{part}");
    }
    assert!(fake.shortcuts.is_empty() && fake.said("No shortcut was made"));
    assert_eq!(fake.rules.len(), 2, "a test install has the firewall rules too");
    // A console built beside it is copied with it.
    let mut fake = Fake::new(false);
    fake.exe = PathBuf::from(BUILD).join("limo.exe");
    fake.files.insert(fake.exe.clone(), "a build".into());
    fake.files.insert(PathBuf::from(BUILD).join(CONSOLE_EXE), "its console".into());
    assert_eq!(install(&mut fake, true), 0);
    assert_eq!(fake.files[&places.install_dir.join(CONSOLE_EXE)], "its console");
}

#[test]
fn installed_again_over_itself_it_keeps_the_family_and_never_doubles_a_rule() {
    let mut fake = Fake::installed(true);
    let family = places().family_file();
    let before = fake.protection.clone();
    assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
    assert!(fake.said(&format!("LiMo {VERSION} is installed: it is installed again over itself.")));
    // The service stopped, LiMo's programs ended (and nothing else), the
    // service updated rather than made, and started again.
    assert!(fake.said("Stopped the service LiMo."));
    assert_eq!(fake.ended, vec![7001, 7002]);
    assert!(fake.processes.iter().any(|p| p.pid == 7003), "a program that is not LiMo's is never ended");
    assert!(fake.changes.contains(&"configure the service".to_string()) && !fake.changes.contains(&"create the service".to_string()));
    assert!(fake.running());
    // The family's file and its folder are as they were.
    assert_eq!(fake.files[&family], "the family");
    assert_eq!(fake.protection, before);
    assert!(fake.said("Kept the data folder") && !fake.changes.iter().any(|c| c.starts_with("make the data folder")));
    // Each rule is there once.
    assert_eq!(fake.rules.len(), 2);
    assert_eq!(fake.changes.iter().filter(|c| c.starts_with("remove the rule")).count(), 2);
    // The log holds both runs.
    assert_eq!(fake.setup_log().matches("The run ended with exit code 0.").count(), 2);
}

#[test]
fn over_the_test_install_it_says_the_version_was_not_recorded_and_switches_the_test_cap_off() {
    // What limo install --dev of an earlier build left: the service, the
    // data folder with the family and the test cap on, and no record.
    let mut fake = Fake::installed(true);
    let places = places();
    fake.files.remove(&places.data_dir.join(RECORD_FILE));
    fake.files.insert(places.settings_file(), "{\n  \"test_cap\": true,\n  \"plain_display\": true\n}\n".into());
    assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
    assert!(fake.said("This replaces an earlier LiMo whose version was not recorded"));
    assert!(fake.said("The test cap of the earlier test install is now off."));
    let settings = fake.settings();
    assert!(!settings.test_cap && settings.plain_display, "the plain display switch is kept");
    assert_eq!(fake.files[&places.family_file()], "the family");
    assert!(fake.files[&places.data_dir.join(RECORD_FILE)].contains(VERSION));
}

#[test]
fn an_older_limo_is_replaced_and_a_newer_one_never() {
    let mut fake = Fake::installed(true);
    let record = places().data_dir.join(RECORD_FILE);
    fake.files.insert(record.clone(), "{\"version\": \"0.0.9\"}".into());
    assert_eq!(install(&mut fake, false), 0);
    assert!(fake.said(&format!("This replaces LiMo 0.0.9 with LiMo {VERSION}.")));
    fake.forget();
    fake.files.insert(record, "{\"version\": \"99.0.0\"}".into());
    assert_eq!(install(&mut fake, false), 2);
    assert!(fake.said(&format!("LiMo 99.0.0 is installed, newer than this LiMo {VERSION}")) && fake.said("Nothing was changed."));
    assert!(fake.changes.is_empty(), "{:?}", fake.changes);
    assert!(fake.running());
}

#[test]
fn a_data_folder_made_by_another_account_is_refused_and_a_changed_access_list_set_back() {
    let places = places();
    // A standard account made the folder first, with LiMo's own access list.
    let mut fake = Fake::new(false);
    fake.folders.insert(places.data_dir.clone());
    fake.protection.insert(places.data_dir.clone(), FolderSeen { owner: ASHA.into(), dacl: DATA_FOLDER_SDDL.into() });
    assert_eq!(install(&mut fake, false), 2);
    assert!(fake.said("is there, but it is not LiMo’s: it was made by the account S-1-5-21-1001") && fake.said("Nothing was changed."));
    assert!(fake.changes.is_empty() && fake.service.is_none());
    // An administrator's folder whose list was opened to every user.
    let mut fake = Fake::installed(true);
    let opened = format!("{DATA_FOLDER_SDDL}(A;OICI;0x1200a9;;;BU)");
    fake.protection.get_mut(&places.data_dir).unwrap().dacl = opened;
    assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
    assert!(fake.said("access list had been changed; it is set back to SYSTEM and Administrators only."));
    assert_eq!(fake.protection[&places.data_dir].dacl, DATA_FOLDER_SDDL);
}

#[test]
fn a_family_file_kept_at_uninstall_is_used_again_by_the_next_install() {
    let mut fake = Fake::installed(true);
    let places = places();
    fake.secrets.push_back(typed(PASSWORD));
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Keep)), 0, "{:#?}", fake.said);
    assert!(fake.service.is_none() && fake.files.contains_key(&places.family_file()));
    fake.forget();
    assert_eq!(install(&mut fake, false), 0, "{:#?}", fake.said);
    assert!(fake.said(&format!("The data folder of an earlier LiMo {VERSION} is here, with the family’s file: it is used again.")));
    assert_eq!(fake.files[&places.family_file()], "the family");
    assert!(fake.changes.contains(&"create the service".to_string()) && fake.running());
}

#[test]
fn a_failing_step_is_said_and_logged_and_the_family_never_left_unlimited() {
    // A new install whose service cannot be made.
    let mut fake = Fake::new(false);
    fake.fail.insert("create the service");
    assert_eq!(install(&mut fake, false), 1);
    assert!(fake.said("Install failed: the service could not be created: create the service was made to fail."));
    assert!(fake.setup_log().contains("The run ended with exit code 1."));
    // Over an install, a service that cannot be updated is started again.
    let mut fake = Fake::installed(true);
    fake.fail.insert("configure the service");
    assert_eq!(install(&mut fake, false), 1);
    assert!(fake.said("Install failed: the service could not be updated"));
    // The firewall's interface failing: everything else is done, the
    // service runs, and the exit code says a step was not done.
    let mut fake = Fake::new(false);
    fake.fail.insert("firewall");
    assert_eq!(install(&mut fake, false), 1);
    assert!(fake.said("NOT DONE: the firewall rule \"LiMo, the family’s phones (TCP 47810)\" could not be added"));
    assert!(fake.said("2 step(s) were not done") && fake.running() && !fake.shortcuts.is_empty());
    // Over an install, the files cannot be copied from a build folder: the
    // service is started again as it was.
    let mut fake = Fake::installed(true);
    fake.exe = PathBuf::from(BUILD).join("limo.exe");
    fake.files.insert(fake.exe.clone(), "a build".into());
    fake.fail.insert("copy");
    assert_eq!(install(&mut fake, false), 1);
    assert!(fake.said("Install failed: copying the program") && fake.said("The service LiMo is started again, as it was."));
    assert!(fake.running());
}

#[test]
fn stop_for_upgrade_stops_the_service_and_ends_limos_programs_only_from_outside_the_folder() {
    let mut fake = Fake::installed(true);
    fake.exe = PathBuf::from(TEMP).join("limo.exe");
    assert_eq!(stop_for_upgrade(&mut fake), 0, "{:#?}", fake.said);
    assert_eq!(fake.service.as_ref().map(|s| s.state.clone()), Some(ServiceRun::Stopped));
    assert_eq!(fake.ended, vec![7001, 7002]);
    assert!(fake.said("Nothing of LiMo runs from C:\\Program Files\\LiMo now"));
    // Nothing else is touched: the rules, the shortcut, the family.
    assert!(fake.changes.iter().all(|c| c == "stop the service" || c.starts_with("end ")), "{:?}", fake.changes);
    // Then the new files, and install, which starts it again.
    fake.exe = places().installed_exe();
    assert_eq!(install(&mut fake, false), 0);
    assert!(fake.running());
    // From inside the program folder it would hold its own file.
    let mut fake = Fake::installed(true);
    assert_eq!(stop_for_upgrade(&mut fake), 2);
    assert!(fake.said("stop-for-upgrade runs from a copy outside") && fake.running() && fake.changes.is_empty());
    // On a PC without LiMo, nothing to do.
    let mut fake = Fake::new(false);
    fake.exe = PathBuf::from(TEMP).join("limo.exe");
    assert_eq!(stop_for_upgrade(&mut fake), 0);
    assert!(fake.said("LiMo is not installed on this PC, so there is nothing to stop.") && fake.changes.is_empty());
}

#[test]
fn uninstall_with_a_family_asks_for_the_password_through_the_door_and_removes_nothing_without_it() {
    let mut fake = Fake::installed(true);
    // No console to type in (an installer that hid it), or Enter alone.
    for typed_nothing in [None, typed("")] {
        fake.secrets.push_back(typed_nothing);
        assert_eq!(uninstall(&mut fake, None), 2, "{:#?}", fake.said);
        assert!(fake.said("No password was typed. Nothing was removed.") && fake.said("limo reset-password"));
        assert!(fake.changes.is_empty(), "{:?}", fake.changes);
        assert!(fake.running() && fake.rules.len() == 2 && fake.files.contains_key(&places().family_file()));
        assert_eq!(fake.lines_asked, 0, "the family's file is not asked about before the password");
        fake.forget();
    }
    // The door's own connection was closed each time.
    assert_eq!(fake.pc.service.door_sessions(), 0);
}

#[test]
fn uninstall_takes_the_right_password_after_a_wrong_one_in_the_doors_own_words() {
    let mut fake = Fake::installed(true);
    let places = places();
    fake.secrets.extend([typed("not the password"), typed(PASSWORD)]);
    fake.lines.push_back(typed("keep"));
    assert_eq!(uninstall(&mut fake, None), 0, "{:#?}", fake.said);
    assert!(fake.said("That password is not right. 4 more tries before a wait.") && fake.said("The guardian password is right."));
    assert!(fake.said("The family’s file is kept."));
    // Then, and only then, everything but the family's file goes.
    assert!(fake.service.is_none() && fake.rules.is_empty() && fake.shortcuts.is_empty());
    assert_eq!(fake.ended, vec![7001, 7002]);
    assert!(fake.files.contains_key(&places.family_file()) && fake.protection.contains_key(&places.data_dir));
    // From the installed copy, the program folder is the installer's to remove.
    assert!(fake.folders.contains(&places.install_dir) && fake.said("the installer removes that folder after it"));
    assert!(fake.said("LiMo is removed."));
    // The log is in the kept data folder; no password anywhere.
    assert!(fake.setup_log().contains("The guardian password is right."));
    fake.assert_no_secret();
}

#[test]
fn wrong_passwords_lead_to_the_familys_wait_and_the_uninstall_stops() {
    let mut fake = Fake::installed(true);
    fake.secrets.extend([typed("not the password"), typed("not the password"), typed("not the password")]);
    assert_eq!(uninstall(&mut fake, None), 2);
    assert!(fake.said("The password was not right 3 times. Nothing was removed."));
    fake.forget();
    // Two more: the fifth sets the family's wait, and the run stops there.
    fake.secrets.extend([typed("not the password"), typed("not the password"), typed(PASSWORD)]);
    assert_eq!(uninstall(&mut fake, None), 2);
    assert!(fake.said("That password is not right. 1 more try before a wait."));
    assert!(fake.said("That password is not right. Too many wrong tries: wait before trying again. Nothing was removed."));
    assert_eq!(fake.secrets.len(), 1, "no try after the wait began");
    assert!(fake.changes.is_empty() && fake.running());
    // During the wait, even the right password waits.
    fake.forget();
    fake.secrets.clear();
    fake.secrets.push_back(typed(PASSWORD));
    assert_eq!(uninstall(&mut fake, None), 2);
    assert!(fake.said("Too many wrong tries. Wait before trying again. Nothing was removed."));
    fake.assert_no_secret();
}

#[test]
fn a_guardian_who_allowed_removal_in_the_settings_is_asked_no_password() {
    let mut fake = Fake::installed(true);
    let console = fake.pc.console(GUARDIAN, 1);
    fake.pc.hello(console);
    fake.pc.sign_in(console);
    fake.pc.run(1);
    fake.pc.done(console, Request::AllowRemoval { password: Secret::new(PASSWORD.into()) });
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Delete)), 0, "{:#?}", fake.said);
    assert!(fake.said("A guardian allowed LiMo’s removal in its settings a moment ago, so no password is needed."));
    assert_eq!(fake.secrets_asked, 0);
    assert!(!fake.folders.contains(&places().data_dir) && !fake.files.contains_key(&places().family_file()));
    // With the data folder gone, the log is in the account's temporary folder.
    let log = &fake.files[&PathBuf::from(TEMP).join(SETUP_LOG_AFTER_DELETE)];
    assert!(log.contains("Deleted C:\\ProgramData\\LiMo, with the family’s file in it."));
}

#[test]
fn with_the_service_stopped_the_password_is_checked_in_the_familys_file_itself() {
    let mut fake = Fake::installed(true);
    fake.service.as_mut().unwrap().state = ServiceRun::Stopped;
    fake.secrets.extend([typed("not the password"), typed(PASSWORD)]);
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Keep)), 0, "{:#?}", fake.said);
    assert!(fake.said("The service does not run, so the guardian password is checked in the family’s file itself."));
    assert!(fake.said("That password is not right. 4 more tries before a wait.") && fake.said("The guardian password is right."));
    assert!(fake.service.is_none());
    fake.assert_no_secret();
}

#[test]
fn an_uninstall_run_from_a_childs_account_is_refused_at_the_door() {
    let mut fake = Fake::installed(true);
    let now = fake.pc.now();
    let asha = fake.pc.hub().add_child("Asha", limo_hub::AgeBand::TenToTwelve, IST, now).unwrap().child_id;
    fake.pc.hub().link_account(ASHA, "asha", AccountLink::Child { child_id: asha }).unwrap();
    fake.door_account = ASHA.into();
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Delete)), 2);
    assert!(fake.said("LiMo’s settings cannot be opened from Asha’s account.") && fake.said("LiMo is removed from a guardian’s account."));
    assert_eq!(fake.secrets_asked, 0);
    assert!(fake.changes.is_empty() && fake.running());
}

#[test]
fn with_no_family_no_password_is_asked_and_the_family_file_question_is_plain() {
    let mut fake = Fake::installed(false);
    fake.lines.extend([typed("maybe"), typed("delete")]);
    assert_eq!(uninstall(&mut fake, None), 0, "{:#?}", fake.said);
    assert!(fake.said("The family’s file has no family yet, so no password is needed."));
    assert_eq!(fake.secrets_asked, 0);
    assert!(fake.said("Type keep or delete.") && fake.said("The family’s file is deleted."));
    assert_eq!(fake.lines_asked, 2);
    // No answer at all keeps it, so that nothing is lost.
    let mut fake = Fake::installed(false);
    fake.lines.push_back(None);
    assert_eq!(uninstall(&mut fake, None), 0);
    assert!(fake.said("No answer could be read, so the family’s file is kept.") && fake.folders.contains(&places().data_dir));
}

#[test]
fn uninstall_from_a_build_folder_removes_the_program_folder_too() {
    let mut fake = Fake::installed(false);
    fake.exe = PathBuf::from(BUILD).join("limo.exe");
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Delete)), 0, "{:#?}", fake.said);
    assert!(!fake.folders.contains(&places().install_dir) && fake.said("Removed C:\\Program Files\\LiMo."));
    assert!(fake.files.keys().all(|file| !file.starts_with(places().install_dir)));
    // Run again, with nothing left: it says so and is done.
    fake.forget();
    assert_eq!(uninstall(&mut fake, None), 0);
    assert!(fake.said("The service LiMo was not installed.") && fake.said("There is no family’s file, so no password is needed."));
}

#[test]
fn a_door_that_does_not_answer_is_knocked_on_again_and_then_nothing_is_removed() {
    let mut fake = Fake::installed(true);
    fake.door_shut = true;
    assert_eq!(uninstall(&mut fake, Some(FamilyFile::Delete)), 1);
    assert_eq!(fake.doors_tried, 1 + limo_windows::installer::DOOR_TRIES);
    assert!(fake.said("LiMo’s settings did not answer") && fake.said("Nothing was removed."));
    assert!(fake.changes.is_empty() && fake.running() && fake.secrets_asked == 0);
}
