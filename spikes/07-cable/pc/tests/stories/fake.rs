//! A fake phone: it answers exactly the commands this program is meant to
//! send, as the test app and Android would, records every one, and fails
//! the test on any other. Its loopback client is real: on `am start` with a
//! loopback port it connects to that port on this PC's own loopback, as the
//! reverse would carry it, sends 256 KiB and checks they come back.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use limo_cable_spike::cable::{Answer, Cable, CableError, Connector, OpenProblem, Via};
use limo_cable_spike::phone::{ACTIVITY, Ask, PACKAGE};
use limo_cable_spike::secret::check;
use limo_cable_spike::ui::Ui;

/// How many bytes the fake app sends through the loopback, as the real one.
pub const LOOPBACK_BYTES: usize = 256 * 1024;

/// What the fake phone holds and how it behaves.
#[derive(Debug)]
pub struct State {
    pub installed: bool,
    pub adb_enabled: bool,
    pub takes: u32,
    pub secret: String,
    pub bytes: u32,
    pub check: String,
    pub caller_uid: i64,
    pub loopbacks: u32,
    pub loopback: String,
    pub loopback_bytes: u64,
    pub reverse: Option<u16>,
    /// Everything asked of it, in order; never a secret's bytes.
    pub sent: Vec<String>,
    /// The last bytes given on standard input, kept for the tests to look for.
    pub last_input: Vec<u8>,
    /// Opening fails so.
    pub open_problem: Option<OpenProblem>,
    /// Answers end their lines with a carriage return, as Google's adb on Windows.
    pub carriage_returns: bool,
    /// The bytes on standard input arrive with a carriage return before each line feed.
    pub input_changed: bool,
    /// Android's answer to an install.
    pub install_answer: String,
    /// The test app never makes its loopback connection.
    pub no_loopback: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            installed: false,
            adb_enabled: true,
            takes: 0,
            secret: "none".to_string(),
            bytes: 0,
            check: "none".to_string(),
            caller_uid: -1,
            loopbacks: 0,
            loopback: "none".to_string(),
            loopback_bytes: 0,
            reverse: None,
            sent: Vec::new(),
            last_input: Vec::new(),
            open_problem: None,
            carriage_returns: false,
            input_changed: false,
            install_answer: "Success".to_string(),
            no_loopback: false,
        }
    }
}

/// The fake phone, shared between the connector and each cable it opens.
#[derive(Clone, Default)]
pub struct Fake(pub Arc<Mutex<State>>);

impl Fake {
    pub fn new() -> Fake {
        Fake::default()
    }

    pub fn with(change: impl FnOnce(&mut State)) -> Fake {
        let fake = Fake::new();
        change(&mut fake.0.lock().unwrap());
        fake
    }

    pub fn sent(&self) -> Vec<String> {
        self.0.lock().unwrap().sent.clone()
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }
}

impl Connector for Fake {
    fn open(&self, via: Via, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
        let mut state = self.state();
        if let Some(problem) = state.open_problem.clone() {
            return Err(problem);
        }
        if !state.adb_enabled {
            return Err(OpenProblem::NoPhone);
        }
        state.sent.push(format!("open {}", via.word()));
        ui.say("> open the fake phone");
        Ok(Box::new(FakeCable { via, fake: self.clone() }))
    }

    fn seen(&self, via: Via) -> Result<Vec<String>, String> {
        let state = self.state();
        Ok(if state.adb_enabled { vec![format!("the fake phone, by {}", via.word())] } else { Vec::new() })
    }
}

struct FakeCable {
    via: Via,
    fake: Fake,
}

fn status_row(s: &State) -> String {
    format!(
        "Row: 0 takes={}, secret={}, bytes={}, check={}, caller_uid={}, why=none, loopbacks={}, loopback={}, \
         loopback_bytes={}, loopback_ms=12, loopback_why=none\n",
        s.takes, s.secret, s.bytes, s.check, s.caller_uid, s.loopbacks, s.loopback, s.loopback_bytes
    )
}

/// The app's loopback client: connects to `port` on this PC, sends, reads
/// back, and records how it went.
fn loopback_run(fake: Fake, port: u16) {
    let result = (|| -> std::io::Result<u64> {
        let stream = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut writer = stream.try_clone()?;
        let sent: Vec<u8> = (0..LOOPBACK_BYTES).map(|i| (i % 253) as u8).collect();
        let to_send = sent.clone();
        let sender = std::thread::spawn(move || {
            let _ = writer.write_all(&to_send);
            let _ = writer.shutdown(Shutdown::Write);
        });
        let mut back = Vec::new();
        let mut reader = stream;
        reader.read_to_end(&mut back)?;
        let _ = sender.join();
        if back == sent { Ok(back.len() as u64) } else { Err(std::io::Error::other("mismatch")) }
    })();
    let mut state = fake.state();
    state.loopbacks += 1;
    match result {
        Ok(bytes) => {
            state.loopback = "ok".to_string();
            state.loopback_bytes = bytes;
        }
        Err(_) => state.loopback = "failed".to_string(),
    }
}

impl FakeCable {
    fn answer(&self, text: String) -> Result<Answer, CableError> {
        let state = self.fake.state();
        let text = if state.carriage_returns { text.replace('\n', "\r\n") } else { text };
        Ok(Answer { text, exit: if self.via == Via::Adb { Some(0) } else { None } })
    }
}

impl Cable for FakeCable {
    fn via(&self) -> Via {
        self.via
    }

    fn shell(&self, ask: &Ask) -> Result<Answer, CableError> {
        let mut state = self.fake.state();
        state.sent.push(format!("shell {}", ask.command()));
        let installed = state.installed;
        let text = match ask {
            Ask::Model => "SM-G988B\n".to_string(),
            Ask::AndroidVersion => "13\n".to_string(),
            Ask::Sdk => "33\n".to_string(),
            Ask::AdbEnabled => if state.adb_enabled { "1\n" } else { "0\n" }.to_string(),
            Ask::IsInstalled => if installed { format!("package:{PACKAGE}\n") } else { String::new() },
            Ask::DoorStatus if installed => status_row(&state),
            Ask::DoorStatus => "Error while accessing provider:ai.lucidloom.limo.spike07.cable\n".to_string(),
            Ask::Start | Ask::StartLoopback(_) if !installed => {
                format!("Starting: Intent {{ cmp={ACTIVITY} }}\nError type 3\nError: Activity class {{{ACTIVITY}}} does not exist.\n")
            }
            Ask::Start => format!("Starting: Intent {{ cmp={ACTIVITY} }}\n"),
            Ask::StartLoopback(port) => {
                if !state.no_loopback && state.reverse == Some(*port) {
                    let fake = self.fake.clone();
                    let port = *port;
                    std::thread::spawn(move || loopback_run(fake, port));
                }
                format!("Starting: Intent {{ cmp={ACTIVITY} (has extras) }}\n")
            }
            Ask::DebuggingOff => {
                state.adb_enabled = false;
                return Err(CableError::Lost("the fake phone switched USB debugging off".to_string()));
            }
            Ask::DoorWrite => panic!("the door is written only with standard input"),
        };
        drop(state);
        self.answer(text)
    }

    fn shell_with_input(&self, ask: &Ask, input: &[u8]) -> Result<Answer, CableError> {
        assert_eq!(*ask, Ask::DoorWrite, "only the door takes standard input");
        let mut state = self.fake.state();
        state.sent.push(format!("input {} ({} bytes)", ask.command(), input.len()));
        state.last_input = input.to_vec();
        if state.installed {
            let arrived: Vec<u8> = if state.input_changed {
                input.iter().flat_map(|&b| if b == b'\n' { vec![b'\r', b'\n'] } else { vec![b] }).collect()
            } else {
                input.to_vec()
            };
            state.takes += 1;
            state.secret = "taken".to_string();
            state.bytes = arrived.len() as u32;
            state.check = check(&arrived);
            state.caller_uid = 2000;
        }
        drop(state);
        self.answer(String::new())
    }

    fn install(&self, apk: &Path) -> Result<String, CableError> {
        let mut state = self.fake.state();
        state.sent.push(format!("install {}", apk.file_name().unwrap().to_string_lossy()));
        if state.install_answer == "Success" {
            state.installed = true;
        }
        Ok(state.install_answer.clone())
    }

    fn uninstall(&self) -> Result<String, CableError> {
        let mut state = self.fake.state();
        state.sent.push(format!("uninstall {PACKAGE}"));
        let was = state.installed;
        state.installed = false;
        Ok(if was { "Success" } else { "Failure [DELETE_FAILED_INTERNAL_ERROR]" }.to_string())
    }

    fn reverse(&self, port: u16) -> Result<(), CableError> {
        let mut state = self.fake.state();
        state.sent.push(format!("reverse {port}"));
        state.reverse = Some(port);
        Ok(())
    }

    fn reverse_remove(&self, port: u16) -> Result<(), CableError> {
        let mut state = self.fake.state();
        state.sent.push(format!("reverse remove {port}"));
        state.reverse = None;
        Ok(())
    }

    fn close(self: Box<Self>) -> Vec<String> {
        self.fake.state().sent.push("close".to_string());
        vec!["Let go of the fake phone.".to_string()]
    }
}
