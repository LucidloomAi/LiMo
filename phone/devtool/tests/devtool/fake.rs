//! A fake phone behind the [`Adb`] trait, and a fake terminal behind
//! [`Ui`]. The phone answers exactly the calls the tool is meant to make,
//! written out here letter for letter, and fails a test on any other: so
//! these tests also pin down what crosses the cable. Its debug receiver
//! does what the README says LiMo's does: it makes the engine
//! from the pairing file with the real phone engine, keeps the state, and
//! deletes the file.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use limo_core::Grant;
use limo_phone::{CallState, Front, PhoneEngine, PhoneFacts, PhonePermissions};
use limo_phone_dev::adb::{ADB_NOT_FOUND, Adb, Call, Reply};
use limo_phone_dev::places::Places;
use limo_phone_dev::ui::{Hidden, Ui};
use limo_phone_dev::{Tool, run};
use zeroize::Zeroizing;

pub const SERIAL: &str = "R5CN70TEST";
pub const PACKAGE: &str = "ai.lucidloom.limo.child";
pub const STATE: &str = "files/limo/state";
pub const PAIRING: &str = "files/limo-dev/pairing.json";
/// 2027-01-15 08:00:00 UTC, 13:30 at home (UTC+05:30).
pub const NOW: i64 = 1_800_000_000;
pub const ZONE: i32 = 19_800;
pub const PIN: &str = "482915";

pub const PROPS: &str = "getprop ro.build.version.release; getprop ro.build.version.sdk; getprop ro.product.manufacturer; getprop ro.product.model";
pub const LISTED: &str = "pm list packages ai.lucidloom.limo.child";
pub const DUMP: &str = "dumpsys package ai.lucidloom.limo.child";
pub const PROBE: &str = "run-as ai.lucidloom.limo.child id -u";
pub const LOOK: &str = "run-as ai.lucidloom.limo.child sh -c 'test -e files/limo/state && echo state; test -e files/limo-dev/pairing.json && echo pairing; true'";
pub const READ: &str = "run-as ai.lucidloom.limo.child sh -c 'test -e files/limo/state || exit 3; cat files/limo/state'";
pub const WRITE: &str = "run-as ai.lucidloom.limo.child sh -c 'umask 077 && mkdir -p files/limo-dev && cat > files/limo-dev/pairing.json.part && mv -f files/limo-dev/pairing.json.part files/limo-dev/pairing.json && wc -c < files/limo-dev/pairing.json'";
pub const REMOVE: &str = "run-as ai.lucidloom.limo.child rm -f files/limo-dev/pairing.json files/limo-dev/pairing.json.part";
pub const PAIR: &str = "am broadcast -a ai.lucidloom.limo.child.dev.PAIR -n ai.lucidloom.limo.child/.dev.DevReceiver --include-stopped-packages --receiver-foreground";
pub const FORGET: &str = "am broadcast -a ai.lucidloom.limo.child.dev.FORGET -n ai.lucidloom.limo.child/.dev.DevReceiver --include-stopped-packages --receiver-foreground";
pub const CLOCK: &str = "date +%s; date +%z";
pub const LOGCAT: [&str; 6] = ["logcat", "-v", "time", "-s", "LiMo:V", "LiMoWatch:V"];

/// LiMo's child form as installed on the fake phone.
#[derive(Debug, Clone)]
pub struct App {
    pub debuggable: bool,
    pub version: String,
}

/// One call the tool made.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub args: Vec<String>,
    pub stdin: Option<Vec<u8>>,
    pub timeout_secs: u64,
}

impl Recorded {
    /// The shell command of a `shell -T` call, if it is one.
    pub fn shell(&self) -> Option<&str> {
        match self.args.as_slice() {
            [_, _, shell, t, command] if shell == "shell" && t == "-T" => Some(command.as_str()),
            _ => None,
        }
    }
}

/// A phone, as adb sees it.
pub struct FakePhone {
    pub adb_missing: bool,
    /// What `adb devices` lists: serial and state.
    pub devices: Vec<(String, String)>,
    pub release: String,
    pub sdk: u32,
    pub app: Option<App>,
    /// LiMo's own files, by their path in its data folder.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The debug receiver is built into the app.
    pub receiver: bool,
    /// run-as broken by the maker, whatever the build.
    pub run_as_broken: bool,
    /// LiMo is a device admin.
    pub admin: bool,
    /// What `adb install` answers, when not "Success".
    pub install_refusal: Option<String>,
    /// The phone's clock.
    pub wall_secs: i64,
    pub zone: String,
    pub elapsed_ms: u64,
    /// What the log holds.
    pub log: Vec<String>,
    /// After this many calls to the phone, it is unplugged.
    pub gone_after: Option<usize>,
    /// This shell command never ends: it is stopped at its timeout.
    pub hangs_on: Option<String>,
    pub calls: Vec<Recorded>,
    pub pauses: u32,
}

impl FakePhone {
    /// A phone attached and allowed, with the debug child form installed
    /// and its receiver built, and nothing kept.
    pub fn ready() -> FakePhone {
        FakePhone {
            adb_missing: false,
            devices: vec![(SERIAL.to_string(), "device".to_string())],
            release: "13".to_string(),
            sdk: 33,
            app: Some(App { debuggable: true, version: "0.1.0".to_string() }),
            files: BTreeMap::new(),
            receiver: true,
            run_as_broken: false,
            admin: false,
            install_refusal: None,
            wall_secs: NOW,
            zone: "+0530".to_string(),
            elapsed_ms: 3_600_000,
            log: Vec::new(),
            gone_after: None,
            hangs_on: None,
            calls: Vec::new(),
            pauses: 0,
        }
    }

    /// Every call but `adb devices`.
    pub fn phone_calls(&self) -> Vec<&Recorded> {
        self.calls.iter().filter(|c| c.args != ["devices"]).collect()
    }

    /// The shell commands sent, in order.
    pub fn shell_commands(&self) -> Vec<String> {
        self.calls.iter().filter_map(|c| c.shell().map(str::to_string)).collect()
    }

    /// True when no call could have changed anything on the phone.
    pub fn changed_nothing(&self) -> bool {
        self.calls.iter().all(|c| {
            c.stdin.is_none()
                && !c.args.iter().any(|a| a == "install" || a == "uninstall")
                && c.shell().is_none_or(|s| ![WRITE, REMOVE, PAIR, FORGET].contains(&s))
        })
    }

    fn run_as(&self) -> Result<(), Reply> {
        match &self.app {
            None => Err(Reply::failed_with(1, "run-as: unknown package: ai.lucidloom.limo.child\n")),
            Some(app) if !app.debuggable => Err(Reply::failed_with(1, "run-as: package not debuggable: ai.lucidloom.limo.child\n")),
            Some(_) if self.run_as_broken => {
                Err(Reply::failed_with(1, "run-as: couldn't stat /data/user/0/ai.lucidloom.limo.child: Permission denied\n"))
            }
            Some(_) => Ok(()),
        }
    }

    /// The receiver: what LiMo's debug build does with the broadcasts.
    fn receive(&mut self, command: &str) {
        let debuggable = self.app.as_ref().is_some_and(|a| a.debuggable);
        if !self.receiver || !debuggable {
            return;
        }
        if command == PAIR {
            // Deleted first, whatever it held, and never read twice.
            let Some(bytes) = self.files.remove(PAIRING) else { return };
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("the pairing file is JSON");
            // A file made more than an hour from the phone's clock is left
            // over, not just pushed: refused.
            let made_at = value["made_at"].as_i64().expect("made_at is a whole number");
            if (made_at - self.wall_secs).abs() > 3_600 {
                return;
            }
            // As the Kotlin side hands it to the bridge: the welcome as JSON
            // bytes, the secret as bytes.
            let welcome_bytes = serde_json::to_vec(&value["welcome"]).unwrap();
            let welcome: Grant = serde_json::from_slice(&welcome_bytes).expect("the welcome is a Grant");
            let secret = unhex(value["code_secret_hex"].as_str().expect("the secret is text"));
            let mut engine = PhoneEngine::paired(&welcome, secret, self.wall_secs).expect("the engine is made");
            self.files.insert(STATE.to_string(), engine.save());
        } else if command == FORGET {
            self.files.remove(STATE);
        }
    }

    fn shell(&mut self, command: &str, stdin: Option<&[u8]>) -> Reply {
        let limo = |phone: &FakePhone| phone.run_as();
        match command {
            PROPS => Reply::ok_with(&format!("{}\n{}\nsamsung\nSM-G988B\n", self.release, self.sdk)),
            LISTED => Reply::ok_with(if self.app.is_some() { "package:ai.lucidloom.limo.child\n" } else { "" }),
            DUMP => match &self.app {
                Some(app) => Reply::ok_with(&format!(
                    "Packages:\n  Package [ai.lucidloom.limo.child] (1a2b3c):\n    versionName={}\n    flags=[ {}HAS_CODE ALLOW_CLEAR_USER_DATA ]\n",
                    app.version,
                    if app.debuggable { "DEBUGGABLE " } else { "" }
                )),
                None => Reply::ok_with("Unable to find package: ai.lucidloom.limo.child\n"),
            },
            PROBE => limo(self).map(|()| Reply::ok_with("10245\n")).unwrap_or_else(|r| r),
            LOOK => limo(self)
                .map(|()| {
                    let mut out = String::new();
                    if self.files.contains_key(STATE) {
                        out.push_str("state\n");
                    }
                    if self.files.contains_key(PAIRING) {
                        out.push_str("pairing\n");
                    }
                    Reply::ok_with(&out)
                })
                .unwrap_or_else(|r| r),
            READ => limo(self)
                .map(|()| match self.files.get(STATE) {
                    Some(bytes) => Reply { code: Some(0), stdout: bytes.clone(), ..Reply::default() },
                    None => Reply::failed_with(3, ""),
                })
                .unwrap_or_else(|r| r),
            WRITE => match limo(self) {
                Ok(()) => {
                    let bytes = stdin.expect("the pairing file comes on standard input").to_vec();
                    let n = bytes.len();
                    self.files.insert(PAIRING.to_string(), bytes);
                    Reply::ok_with(&format!("{n}\n"))
                }
                Err(r) => r,
            },
            REMOVE => match limo(self) {
                Ok(()) => {
                    self.files.remove(PAIRING);
                    Reply::ok_with("")
                }
                Err(r) => r,
            },
            PAIR | FORGET => {
                self.receive(command);
                Reply::ok_with(&format!(
                    "Broadcasting: Intent {{ act={} flg=0x10000020 cmp=ai.lucidloom.limo.child/.dev.DevReceiver }}\nBroadcast completed: result=0\n",
                    if command == PAIR { "ai.lucidloom.limo.child.dev.PAIR" } else { "ai.lucidloom.limo.child.dev.FORGET" }
                ))
            }
            CLOCK => Reply::ok_with(&format!("{}\n{}\n", self.wall_secs, self.zone)),
            other => panic!("the fake phone was sent a shell command it does not know: {other}"),
        }
    }

    /// LiMo's engine as the phone keeps it, used for `secs` seconds with
    /// this app in front, a call a second, then kept again: what LiMo's
    /// host would do.
    pub fn use_app(&mut self, package: &str, secs: u64) {
        self.with_engine(|engine, phone| {
            for _ in 0..=secs {
                engine.tick(&phone.facts(true, Front::app(package)));
                phone.wall_secs += 1;
                phone.elapsed_ms += 1_000;
            }
            engine.tick(&phone.facts(false, Front::Home));
        });
    }

    /// The facts LiMo's host would give at this moment.
    pub fn facts(&self, screen_on: bool, front: Front) -> PhoneFacts {
        PhoneFacts {
            wall_secs: self.wall_secs,
            elapsed_ms: self.elapsed_ms,
            boot_count: 7,
            utc_offset_secs: ZONE,
            hours_24: true,
            auto_time: true,
            interactive: screen_on,
            keyguard_locked: !screen_on,
            call: CallState::Idle,
            front,
            in_reach: false,
            prodded: false,
            permissions: PhonePermissions::ALL,
        }
    }

    /// Loads LiMo's kept engine, lets `f` act on it, and keeps it again.
    pub fn with_engine(&mut self, f: impl FnOnce(&mut PhoneEngine, &mut FakePhone)) {
        let bytes = self.files.get(STATE).expect("LiMo keeps a state").clone();
        let mut engine = PhoneEngine::load(&bytes);
        assert_eq!(engine.load_problem(), None);
        f(&mut engine, self);
        let saved = engine.save();
        self.files.insert(STATE.to_string(), saved);
    }
}

impl Adb for FakePhone {
    fn ready(&mut self) -> Result<String, String> {
        if self.adb_missing { Err(ADB_NOT_FOUND.to_string()) } else { Ok("adb 1.0.41 (the fake)".to_string()) }
    }

    fn run(&mut self, call: Call<'_>) -> Result<Reply, String> {
        self.calls.push(Recorded {
            args: call.args.clone(),
            stdin: call.stdin.map(<[u8]>::to_vec),
            timeout_secs: call.timeout.as_secs(),
        });
        let args: Vec<&str> = call.args.iter().map(String::as_str).collect();
        if args == ["devices"] {
            let mut out = "List of devices attached\n".to_string();
            for (serial, state) in &self.devices {
                out.push_str(&format!("{serial}\t{state}\n"));
            }
            return Ok(Reply::ok_with(&out));
        }
        let ["-s", serial, rest @ ..] = args.as_slice() else { panic!("a call without -s: {args:?}") };
        let gone = self.gone_after.is_some_and(|n| self.phone_calls().len() > n);
        if let ["shell", "-T", command] = rest
            && !gone
            && self.hangs_on.as_deref() == Some(*command)
        {
            return Ok(Reply { timed_out: true, ..Reply::default() });
        }
        if gone || !self.devices.iter().any(|(s, st)| s == serial && st == "device") {
            return Ok(Reply::failed_with(1, &format!("adb: error: device '{serial}' not found\n")));
        }
        Ok(match rest {
            ["shell", "-T", command] => self.shell(command, call.stdin),
            ["install", "-r", "-t", _path] => match &self.install_refusal {
                Some(refusal) => Reply::failed_with(1, refusal),
                None => {
                    // Replacing LiMo keeps its files; a new install has none.
                    if self.app.is_none() {
                        self.files.clear();
                    }
                    self.app = Some(App { debuggable: true, version: "0.1.0".to_string() });
                    Reply::ok_with("Performing Streamed Install\nSuccess\n")
                }
            },
            ["uninstall", PACKAGE] => {
                if self.admin {
                    Reply::failed_with(1, "Failure [DELETE_FAILED_DEVICE_POLICY_MANAGER]\n")
                } else if self.app.is_none() {
                    Reply::failed_with(1, "Failure [DELETE_FAILED_INTERNAL_ERROR]\nUnknown package: ai.lucidloom.limo.child\n")
                } else {
                    self.app = None;
                    self.files.clear();
                    Reply::ok_with("Success\n")
                }
            }
            rest if rest == LOGCAT => {
                if let Some(on_line) = call.on_line {
                    for line in &self.log {
                        on_line(line);
                    }
                }
                // As if the limit was reached.
                Reply { timed_out: true, ..Reply::default() }
            }
            other => panic!("the fake phone was sent a call it does not know: {other:?}"),
        })
    }

    fn pause(&mut self, _d: Duration) {
        self.pauses += 1;
    }
}

fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2) && text.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{text}");
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

/// A terminal that keeps every line, and types what the test gives it.
#[derive(Default)]
pub struct FakeUi {
    pub lines: Vec<String>,
    pub typed: VecDeque<String>,
    pub prompts: Vec<String>,
    pub not_a_terminal: bool,
}

impl Ui for FakeUi {
    fn say(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    fn hidden(&mut self, prompt: &str) -> Hidden {
        self.prompts.push(prompt.to_string());
        if self.not_a_terminal {
            return Hidden::NotATerminal;
        }
        match self.typed.pop_front() {
            Some(t) => Hidden::Typed(Zeroizing::new(t)),
            None => Hidden::Nothing,
        }
    }
}

/// The phone, the terminal and the tool's folder, for one story.
pub struct Bench {
    pub phone: FakePhone,
    pub ui: FakeUi,
    pub dir: tempfile::TempDir,
    /// This PC's clock.
    pub now: i64,
}

impl Bench {
    /// A ready phone and an empty folder.
    pub fn new() -> Bench {
        Bench { phone: FakePhone::ready(), ui: FakeUi::default(), dir: tempfile::tempdir().unwrap(), now: NOW }
    }

    pub fn places(&self) -> Places {
        Places::in_folder(self.folder())
    }

    pub fn folder(&self) -> PathBuf {
        self.dir.path().join(limo_phone_dev::places::FOLDER_NAME)
    }

    /// Runs one command line; the exit code. The lines it printed are
    /// `ui.lines`, after those of earlier commands.
    pub fn run(&mut self, line: &str) -> i32 {
        let argv: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        self.run_args(&argv)
    }

    pub fn run_args(&mut self, argv: &[String]) -> i32 {
        let now = self.now;
        let clock = move || now;
        let places = self.places();
        let mut tool = Tool { adb: &mut self.phone, ui: &mut self.ui, places, clock: &clock };
        run(&mut tool, argv)
    }

    /// `setup` with the PIN typed twice.
    pub fn setup(&mut self, line: &str) -> i32 {
        self.ui.typed.push_back(PIN.to_string());
        self.ui.typed.push_back(PIN.to_string());
        self.run(line)
    }

    /// Everything printed so far, one line each.
    pub fn printed(&self) -> String {
        self.ui.lines.join("\n")
    }

    /// Forgets what was printed and what was called.
    pub fn clear(&mut self) {
        self.ui.lines.clear();
        self.phone.calls.clear();
    }

    /// The tool's log, or nothing.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.places().log_file()).unwrap_or_default()
    }

    /// The phone's engine, from what LiMo keeps.
    pub fn engine(&self) -> PhoneEngine {
        PhoneEngine::load(self.phone.files.get(STATE).expect("LiMo keeps a state"))
    }
}
