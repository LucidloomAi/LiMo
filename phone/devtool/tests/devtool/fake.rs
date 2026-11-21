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
use limo_phone_dev::usb::{UsbDevice, UsbLook};
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
/// Sent through `exec-out`, never `shell`.
pub const READ: &str = "run-as ai.lucidloom.limo.child sh -c 'if test -e files/limo/state; then echo limo-dev-state; cat files/limo/state; else echo limo-dev-none; fi'";
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

    /// The command of an `exec-out` call, if it is one.
    pub fn exec_out(&self) -> Option<&str> {
        match self.args.as_slice() {
            [_, _, exec, command] if exec == "exec-out" => Some(command.as_str()),
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
    /// The child's name the receiver took, kept beside the state.
    pub child_name: Option<String>,
    /// The receiver refuses every pairing file with this word, as an app
    /// built from another version might.
    pub refuse_with: Option<String>,
    /// `exec-out` changes the bytes too, as `shell` does: the state cannot
    /// be read back, whatever LiMo did.
    pub mangles_exec_out: bool,
    /// LiMo's log, read once, gives nothing (a `logcat` that does not take
    /// the time it is given, say).
    pub log_silent: bool,
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
            child_name: None,
            refuse_with: None,
            mangles_exec_out: false,
            log_silent: false,
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
            let Some(bytes) = self.files.remove(PAIRING) else {
                self.say("dev pair no_file");
                return;
            };
            if let Some(why) = self.refuse_with.clone() {
                self.say(&format!("dev pair refused why={why}"));
                return;
            }
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("the pairing file is JSON");
            // A file made more than an hour from the phone's clock is left
            // over, not just pushed: refused.
            let made_at = value["made_at"].as_i64().expect("made_at is a whole number");
            if (made_at - self.wall_secs).abs() > 3_600 {
                self.say("dev pair refused why=old_file");
                return;
            }
            // The child's name is required: the welcome carries none.
            let Some(child_name) = value["child_name"].as_str().filter(|n| !n.is_empty()).map(str::to_string) else {
                self.say("dev pair refused why=no_child_name");
                return;
            };
            // As the Kotlin side hands it to the bridge: the welcome as JSON
            // bytes, the secret as bytes.
            let welcome_bytes = serde_json::to_vec(&value["welcome"]).unwrap();
            let welcome: Grant = serde_json::from_slice(&welcome_bytes).expect("the welcome is a Grant");
            let secret = unhex(value["code_secret_hex"].as_str().expect("the secret is text"));
            let mut engine = PhoneEngine::paired(&welcome, secret, self.wall_secs).expect("the engine is made");
            self.files.insert(STATE.to_string(), engine.save());
            // Kept beside the state, never inside the engine's.
            self.child_name = Some(child_name);
            self.say("dev pair taken");
        } else if command == FORGET {
            self.files.remove(STATE);
            self.child_name = None;
            self.say("dev forget done");
        }
    }

    /// A line in the phone's log with the tag `LiMo`, as the receiver
    /// writes one, in logcat's `time` format.
    fn say(&mut self, message: &str) {
        let n = self.log.len();
        self.log.push(format!("11-20 13:30:{:02}.000 I/LiMo    ( 4242): {message}", n % 60));
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
            // Answered through shell too, as the phone would: then its line
            // ends come back changed, as the first real run saw.
            READ => self.state_answer(),
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

    /// What the phone's shell prints for [`READ`]: a first line saying
    /// whether the state follows, then its bytes as kept; or run-as's
    /// refusal.
    fn state_answer(&self) -> Reply {
        match self.run_as() {
            Err(refused) => refused,
            Ok(()) => match self.files.get(STATE) {
                Some(bytes) => {
                    let mut out = b"limo-dev-state\n".to_vec();
                    out.extend_from_slice(bytes);
                    Reply { code: Some(0), stdout: out, ..Reply::default() }
                }
                None => Reply::ok_with("limo-dev-none\n"),
            },
        }
    }

    /// `adb exec-out`, as the real one on Windows: one stream with the
    /// command's errors on it too, its bytes as they are, and an exit code
    /// of 0 whatever happened.
    fn exec_out(&mut self, command: &str) -> Reply {
        let answer = match command {
            READ => self.state_answer(),
            other => panic!("the fake phone was sent an exec-out command it does not know: {other}"),
        };
        let mut stdout = answer.stdout;
        stdout.extend_from_slice(answer.stderr.as_bytes());
        if self.mangles_exec_out {
            stdout = crlf_bytes(&stdout);
        }
        Reply { code: Some(0), stdout, stderr: String::new(), timed_out: false }
    }

    /// A guardian finishes setup on the phone, as its last setup screen does:
    /// until then LiMo counts nothing and covers nothing.
    pub fn finish_setup(&mut self) {
        self.with_engine(|engine, phone| {
            assert_eq!(engine.finish_setup(phone.wall_secs), Ok(true));
        });
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
            guarded: None,
            usb_debugging: None,
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
            return Ok(crlf(Reply::ok_with(&out)));
        }
        let ["-s", serial, rest @ ..] = args.as_slice() else { panic!("a call without -s: {args:?}") };
        let gone = self.gone_after.is_some_and(|n| self.phone_calls().len() > n);
        if let ["shell", "-T", command] | ["exec-out", command] = rest
            && !gone
            && self.hangs_on.as_deref() == Some(*command)
        {
            return Ok(Reply { timed_out: true, ..Reply::default() });
        }
        if gone || !self.devices.iter().any(|(s, st)| s == serial && st == "device") {
            return Ok(crlf(Reply::failed_with(1, &format!("adb: error: device '{serial}' not found\n"))));
        }
        // Every answer but exec-out's comes back with each line feed made
        // a carriage return and a line feed, as adb on Windows gave them in
        // the first real run.
        let reply = match rest {
            ["exec-out", command] => return Ok(self.exec_out(command)),
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
                    self.child_name = None;
                    Reply::ok_with("Success\n")
                }
            }
            // The log read once for LiMo's tag, since a moment: every line of
            // the fake's log is taken to be that recent.
            ["logcat", "-d", "-v", "time", "-t", since, "-s", "LiMo:V"] => {
                assert!(since.ends_with(".000") && since[..since.len() - 4].parse::<i64>().is_ok(), "{since}");
                let lines: Vec<&String> =
                    self.log.iter().filter(|l| l.contains(" I/LiMo ") && !self.log_silent).collect();
                Reply::ok_with(&lines.iter().map(|l| format!("{l}\n")).collect::<String>())
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
        };
        Ok(crlf(reply))
    }

    fn pause(&mut self, _d: Duration) {
        self.pauses += 1;
    }
}

/// Each line feed made a carriage return and a line feed; one already
/// after a carriage return is left.
pub fn crlf_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 8);
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
            out.push(b'\r');
        }
        out.push(*b);
    }
    out
}

/// A reply as adb on Windows gives it through `shell` and its other
/// commands but `exec-out`.
fn crlf(reply: Reply) -> Reply {
    let stderr = String::from_utf8(crlf_bytes(reply.stderr.as_bytes())).unwrap();
    Reply { stdout: crlf_bytes(&reply.stdout), stderr, ..reply }
}

fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2) && text.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{text}");
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

/// The PC's USB ports as Windows lists them: a list given by the test, or
/// a list that cannot be read; and how many times it was looked at.
pub struct FakeUsb {
    pub seen: Result<Vec<UsbDevice>, String>,
    pub looked: u32,
}

impl Default for FakeUsb {
    /// Windows sees nothing that looks like a phone.
    fn default() -> FakeUsb {
        FakeUsb { seen: Ok(Vec::new()), looked: 0 }
    }
}

impl UsbLook for FakeUsb {
    fn phones_seen(&mut self) -> Result<Vec<UsbDevice>, String> {
        self.looked += 1;
        self.seen.clone()
    }
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
    pub usb: FakeUsb,
    pub ui: FakeUi,
    pub dir: tempfile::TempDir,
    /// This PC's clock.
    pub now: i64,
}

impl Bench {
    /// A ready phone and an empty folder.
    pub fn new() -> Bench {
        Bench {
            phone: FakePhone::ready(),
            usb: FakeUsb::default(),
            ui: FakeUi::default(),
            dir: tempfile::tempdir().unwrap(),
            now: NOW,
        }
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
        let mut tool = Tool { adb: &mut self.phone, usb: &mut self.usb, ui: &mut self.ui, places, clock: &clock };
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
