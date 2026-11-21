//! A fake phone at the far end of a fake cable, for the tests of this crate
//! and of the console (feature `fake`; never part of the product).
//!
//! It behaves as the test phone did on 21 November 2026, at both ends:
//!
//! - **The PC's side** ([`FakePhone`] is a [`Cable`]): what Windows lists
//!   for a cable that only charges, USB debugging off, no driver, a maker's
//!   driver LiMo cannot open, or Samsung's driver package, which binds
//!   WinUSB to the debugging interface under Samsung's own composite driver
//!   (`dg_ssudbus`) and offers the Android USB interface class: the one the
//!   phone had.
//! - **The phone's side**: an adbd that speaks the protocol as AOSP's does,
//!   over an in memory wire shaped like USB (one transfer at a time, a
//!   message's header and its data apart). It asks for the key, checks the
//!   signature against the keys it allows, shows an unknown key and lets the
//!   "person" allow it or not, answers LiMo's commands as Android did,
//!   installs with no question, takes an offer through LiMo's door, hands
//!   `am start` to a screen already open as Android does, keeps reverses,
//!   switches USB debugging off and goes, and plays LiMo's app connecting to
//!   a reversed port ([`FakePhone::app_connects`]). adb's streams have no
//!   half close, here as there: the app's close ends the stream both ways.
//!
//! Every command and service it is asked for is recorded ([`Seen`]); one
//! it does not know is refused, and recorded as such.

#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::{BigUint, RsaPublicKey};

use crate::commands::{ACTIVITY, Ask, PACKAGE};
use crate::key::AdbKey;
use crate::open::{Cable, Interface, Look};
use crate::phone::{Knock, OpenProblem, Phone, Waits};
use crate::protocol::{
    AUTH, AUTH_RSAPUBLICKEY, AUTH_SIGNATURE, AUTH_TOKEN, CLSE, CNXN, Message, OKAY, OPEN, SHA1_PREFIX, VERSION, WRTE,
    Wire, shell_id, shell_packet,
};
use crate::usb::{Kind, UsbDevice};

/// The packet size of the fake USB link, a high speed bulk pipe's.
pub const PACKET: usize = 512;

/// The cable in the fake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableKind {
    /// One that carries data.
    Data,
    /// One that only charges: Windows sees nothing.
    ChargeOnly,
}

/// The driver Windows bound to the phone's debugging interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// Samsung's driver package: WinUSB on the interface, Samsung's
    /// composite driver above it, the Android class offered. What the test
    /// phone had.
    SamsungWinUsb,
    /// Windows' own WinUSB through the phone's own descriptors, Windows'
    /// own composite driver above it.
    WindowsWinUsb,
    /// A maker's own driver, not WinUSB: the Android class offered, but
    /// LiMo cannot open it.
    MakersOwn,
    /// No driver at all.
    None,
}

/// What the person at the phone does when it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Person {
    /// Taps Allow (with "Always allow", so the key is kept).
    Allows,
    /// Is not there, or does not tap.
    Away,
}

/// LiMo's child form on the fake phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeLimo {
    /// Its version code.
    pub version_code: u64,
    /// Its version name.
    pub version_name: String,
    /// Signed with another key than the APKs installed over it.
    pub other_key: bool,
}

/// What the fake phone was asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// Every shell command, in order.
    pub commands: Vec<String>,
    /// Every service opened, in order (shell commands under their service).
    pub services: Vec<String>,
    /// Services and commands refused, because the fake does not know them.
    pub refused: Vec<String>,
    /// How many times it asked "Allow USB debugging?": each time the PC
    /// sent it a public key it did not know.
    pub prompts: u32,
    /// The kind of every `AUTH` message the PC sent, in order: 2 a
    /// signature, 3 a public key.
    pub auth: Vec<u32>,
    /// How many installs it took.
    pub installs: u32,
    /// The bytes of every install it took, in order.
    pub installed: Vec<Vec<u8>>,
    /// The largest data in one message from the PC.
    pub largest_write: usize,
}

/// The fake phone, and the cable to it.
#[derive(Debug)]
pub struct FakeState {
    /// Plugged in.
    pub plugged: bool,
    /// The cable.
    pub cable: CableKind,
    /// USB debugging on.
    pub debugging: bool,
    /// The driver Windows bound.
    pub driver: Driver,
    /// Another program holds the interface (Android Studio's adb server).
    pub held_by_another: bool,
    /// The keys the phone allows without asking.
    pub known_keys: Vec<RsaPublicKey>,
    /// What the person does when asked.
    pub person: Person,
    /// The maker, as Android names it.
    pub manufacturer: String,
    /// The model.
    pub model: String,
    /// The Android version.
    pub android: String,
    /// The API level.
    pub sdk: u32,
    /// LiMo on the phone.
    pub limo: Option<FakeLimo>,
    /// LiMo's screen is open.
    pub screen_open: bool,
    /// Every offer LiMo's door took, in order.
    pub offers: Vec<Vec<u8>>,
    /// The ports reversed now.
    pub reverses: Vec<u16>,
    /// The most data one of its messages carries.
    pub max_data: u32,
    /// What it was asked.
    pub seen: Seen,
    /// Bytes written over a file the moment an install's stream opens: the
    /// file LiMo checked changes after LiMo told the phone its size and
    /// before the bytes go. Once.
    pub change_on_install: Option<(PathBuf, Vec<u8>)>,
}

impl FakeState {
    /// The phone knows `key` already, as one that allowed this PC before:
    /// it connects with no question.
    pub fn knows(&mut self, key: &AdbKey) {
        self.known_keys.push(RsaPublicKey::from(key.private()));
    }
}

impl Default for FakeState {
    /// The test phone: a Galaxy S20 Ultra on Android 13, on Samsung's
    /// driver package, USB debugging on, LiMo not installed, a person there
    /// who allows.
    fn default() -> Self {
        FakeState {
            plugged: true,
            cable: CableKind::Data,
            debugging: true,
            driver: Driver::SamsungWinUsb,
            held_by_another: false,
            known_keys: Vec::new(),
            person: Person::Allows,
            manufacturer: "samsung".to_string(),
            model: "SM-G988B".to_string(),
            android: "13".to_string(),
            sdk: 33,
            limo: None,
            screen_open: false,
            offers: Vec::new(),
            reverses: Vec::new(),
            max_data: 256 * 1024,
            seen: Seen::default(),
            change_on_install: None,
        }
    }
}

/// The fake phone and its cable: a [`Cable`] for the console's side.
#[derive(Debug, Default)]
pub struct FakePhone {
    /// Its state, which a test may change at any time (unplug it, switch
    /// USB debugging off, swap the driver).
    pub state: Arc<Mutex<FakeState>>,
    running: Mutex<Vec<FakeAdbd>>,
}

impl FakePhone {
    /// The test phone as it was.
    pub fn new() -> FakePhone {
        FakePhone::default()
    }

    /// The test phone, changed by `change`.
    pub fn with(change: impl FnOnce(&mut FakeState)) -> FakePhone {
        let phone = FakePhone::new();
        change(&mut phone.state());
        phone
    }

    /// Its state, locked.
    pub fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What it was asked so far.
    pub fn seen(&self) -> Seen {
        self.state().seen.clone()
    }

    /// LiMo's app on the phone connects to `port` on the phone's own
    /// loopback: through a reverse, that reaches 127.0.0.1:`port` on the PC.
    /// Refused when no reverse carries the port, as Android refuses it.
    pub fn app_connects(&self, port: u16) -> Result<AppSocket, String> {
        if !self.state().reverses.contains(&port) {
            return Err(format!("nothing listens on the phone's 127.0.0.1:{port}"));
        }
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let adbd = running.last().ok_or_else(|| "the phone is not connected".to_string())?;
        let (reply, answer) = mpsc::channel();
        adbd.control.send(Control::AppConnects { port, reply }).map_err(|_| "the phone is gone".to_string())?;
        answer.recv_timeout(Duration::from_secs(10)).map_err(|_| "the PC did not take the connection".to_string())?
    }

    fn interface(&self) -> Option<Interface> {
        let s = self.state();
        if !s.plugged || s.cable == CableKind::ChargeOnly || !s.debugging {
            return None;
        }
        let interface = |driver: &str, above: &str| Interface {
            name: Some(if s.manufacturer == "samsung" { "SAMSUNG Android ADB Interface" } else { "ADB Interface" }.to_string()),
            driver: Some(driver.to_string()),
            above: Some(above.to_string()),
        };
        match s.driver {
            Driver::SamsungWinUsb => Some(interface("WINUSB", "dg_ssudbus")),
            Driver::WindowsWinUsb => Some(interface("WinUSB", "usbccgp")),
            Driver::MakersOwn => Some(interface("makersadb", "usbccgp")),
            Driver::None => None,
        }
    }
}

impl Cable for FakePhone {
    fn look(&mut self) -> Look {
        let s = self.state();
        let mut windows = Vec::new();
        if s.plugged && s.cable == CableKind::Data {
            windows.push(UsbDevice { kind: Kind::PortableDevice, name: Some("Galaxy S20 Ultra 5G".to_string()), bus: "USB".to_string(), working: true });
            if s.debugging {
                windows.push(UsbDevice {
                    kind: Kind::DebugInterface,
                    name: Some("SAMSUNG Android ADB Interface".to_string()),
                    bus: "USB".to_string(),
                    working: s.driver != Driver::None,
                });
            }
        }
        drop(s);
        Look { windows: Ok(windows), interfaces: Ok(self.interface().into_iter().collect()) }
    }

    fn open(&mut self, key: &AdbKey, waits: Waits, knock: Knock<'_>) -> Result<Phone, OpenProblem> {
        let Some(interface) = self.interface() else { return Err(OpenProblem::NoPhone) };
        if self.state().held_by_another {
            return Err(OpenProblem::Busy);
        }
        if !interface.on_winusb() {
            return Err(OpenProblem::DriverNotUsable("WinUsb_Initialize refused it, so its driver is not WinUSB".to_string()));
        }
        let (adbd, wire) = FakeAdbd::start(self.state.clone());
        self.running.lock().unwrap_or_else(|e| e.into_inner()).push(adbd);
        Phone::connect(Box::new(wire), key, waits, knock)
    }
}

/// The PC's end of the fake wire.
struct HostWire {
    to_phone: Mutex<Sender<Vec<u8>>>,
    from_phone: Mutex<(Receiver<Vec<u8>>, VecDeque<u8>)>,
}

impl Wire for HostWire {
    fn packet(&self) -> usize {
        PACKET
    }

    fn read(&self, want: usize, wait: Duration) -> Result<Option<Vec<u8>>, String> {
        let mut guard = self.from_phone.lock().map_err(|_| "the wire's lock failed".to_string())?;
        let (rx, rest) = &mut *guard;
        if rest.is_empty() {
            match rx.recv_timeout(wait) {
                Ok(transfer) if transfer.is_empty() => return Ok(Some(Vec::new())),
                Ok(transfer) => rest.extend(transfer),
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => return Err("the phone is gone".to_string()),
            }
        }
        let n = want.min(rest.len());
        Ok(Some(rest.drain(..n).collect()))
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.to_phone
            .lock()
            .map_err(|_| "the wire's lock failed".to_string())?
            .send(bytes.to_vec())
            .map_err(|_| "the phone is gone".to_string())
    }
}

/// What the app sends.
enum AppOut {
    Data(Vec<u8>),
    Close,
}

/// LiMo's app's connection through a reverse, on the fake phone.
pub struct AppSocket {
    to_pc: Sender<AppOut>,
    from_pc: Receiver<Vec<u8>>,
    pending: Vec<u8>,
}

impl std::fmt::Debug for AppSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppSocket")
    }
}

impl AppSocket {
    /// Sends bytes to the PC.
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.to_pc.send(AppOut::Data(bytes.to_vec())).map_err(|_| "the connection is closed".to_string())
    }

    /// Reads exactly `n` bytes, waiting at most `wait` for each part.
    pub fn read_exact(&mut self, n: usize, wait: Duration) -> Result<Vec<u8>, String> {
        while self.pending.len() < n {
            match self.from_pc.recv_timeout(wait) {
                Ok(bytes) => self.pending.extend(bytes),
                Err(RecvTimeoutError::Timeout) => return Err(format!("only {} of {n} bytes came in time", self.pending.len())),
                Err(RecvTimeoutError::Disconnected) => return Err(format!("the connection closed after {} of {n} bytes", self.pending.len())),
            }
        }
        Ok(self.pending.drain(..n).collect())
    }

    /// Whatever comes within `wait`, or None.
    pub fn read(&mut self, wait: Duration) -> Option<Vec<u8>> {
        if !self.pending.is_empty() {
            return Some(std::mem::take(&mut self.pending));
        }
        self.from_pc.recv_timeout(wait).ok()
    }

    /// Closes the connection, both ways: adb has no half close.
    pub fn close(self) {
        let _ = self.to_pc.send(AppOut::Close);
    }
}

/// What a test asks the running fake adbd to do.
enum Control {
    AppConnects { port: u16, reply: Sender<Result<AppSocket, String>> },
    Stop,
}

/// One running fake adbd, for one connection.
#[derive(Debug)]
struct FakeAdbd {
    control: Sender<Control>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for FakeAdbd {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl FakeAdbd {
    fn start(state: Arc<Mutex<FakeState>>) -> (FakeAdbd, HostWire) {
        let (to_phone, from_host) = mpsc::channel();
        let (to_host, from_phone) = mpsc::channel();
        let (control, commands) = mpsc::channel();
        let wire = HostWire { to_phone: Mutex::new(to_phone), from_phone: Mutex::new((from_phone, VecDeque::new())) };
        let thread = std::thread::spawn(move || Adbd::new(state, to_host).run(from_host, commands));
        (FakeAdbd { control, thread: Some(thread) }, wire)
    }
}

enum Service {
    Shell { command: String, input: Vec<u8> },
    Install { size: usize, bytes: Vec<u8> },
    Text,
    App { to_app: Sender<Vec<u8>>, from_app: Receiver<AppOut> },
}

struct Stream {
    host: u32,
    kind: Service,
    outbox: VecDeque<Vec<u8>>,
    waiting: bool,
    close_after: bool,
    closed: bool,
}

struct Adbd {
    state: Arc<Mutex<FakeState>>,
    to_host: Sender<Vec<u8>>,
    streams: HashMap<u32, Stream>,
    next_id: u32,
    token: Vec<u8>,
    tokens: u8,
    pending_app: HashMap<u32, (Sender<Result<AppSocket, String>>, AppSocket)>,
    /// A key shown while nobody allowed it: allowed if the person does so
    /// later, while the PC still waits.
    pending_key: Option<RsaPublicKey>,
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let Some(v) = value(c) else { continue };
        buffer = (buffer << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

impl Adbd {
    fn new(state: Arc<Mutex<FakeState>>, to_host: Sender<Vec<u8>>) -> Adbd {
        Adbd { state, to_host, streams: HashMap::new(), next_id: 1000, token: Vec::new(), tokens: 0, pending_app: HashMap::new(), pending_key: None }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// As adbd on USB: the header in one transfer, the data in the next.
    fn send(&self, message: Message) {
        let _ = self.to_host.send(message.header().to_vec());
        if !message.data.is_empty() {
            let _ = self.to_host.send(message.data);
        }
    }

    fn run(mut self, from_host: Receiver<Vec<u8>>, commands: Receiver<Control>) {
        loop {
            match commands.try_recv() {
                Ok(Control::Stop) | Err(TryRecvError::Disconnected) => return,
                Ok(Control::AppConnects { port, reply }) => self.app_connects(port, reply),
                Err(TryRecvError::Empty) => {}
            }
            self.pump();
            {
                // Unplugged, or USB debugging switched off: the interface goes,
                // at once, as a real phone's does, whatever it was doing.
                let s = self.state();
                if !s.debugging || !s.plugged {
                    return;
                }
            }
            // The person allows a key shown earlier, while the PC waits.
            if self.pending_key.is_some() && self.state().person == Person::Allows {
                if let Some(key) = self.pending_key.take() {
                    self.state().known_keys.push(key);
                }
                self.hello();
            }
            let message = match read_host(&from_host, Duration::from_millis(10)) {
                Ok(Some(m)) => m,
                Ok(None) => continue,
                Err(()) => return,
            };
            self.handle(message);
            let s = self.state();
            if !s.debugging || !s.plugged {
                // USB debugging off, or unplugged: the interface goes.
                return;
            }
        }
    }

    /// The app opens a stream to the PC; its socket is handed over once the
    /// PC takes the stream, or the refusal once it refuses it.
    fn app_connects(&mut self, port: u16, reply: Sender<Result<AppSocket, String>>) {
        self.next_id += 1;
        let id = self.next_id;
        let (to_app, from_pc) = mpsc::channel();
        let (to_pc, from_app) = mpsc::channel();
        self.streams.insert(
            id,
            Stream { host: 0, kind: Service::App { to_app, from_app }, outbox: VecDeque::new(), waiting: false, close_after: false, closed: false },
        );
        self.pending_app.insert(id, (reply, AppSocket { to_pc, from_pc, pending: Vec::new() }));
        let mut name = format!("tcp:{port}").into_bytes();
        name.push(0);
        self.send(Message::new(OPEN, id, 0, &name));
    }

    /// Sends what is ready: one message a stream at a time, each waiting for
    /// the PC's OKAY before the next.
    fn pump(&mut self) {
        let max = self.state().max_data as usize;
        let mut to_send = Vec::new();
        for (id, s) in self.streams.iter_mut() {
            if s.host == 0 || s.closed {
                continue;
            }
            if let Service::App { from_app, .. } = &s.kind
                && s.outbox.is_empty()
                && !s.close_after
            {
                match from_app.try_recv() {
                    Ok(AppOut::Data(bytes)) => s.outbox.extend(bytes.chunks(max).map(<[u8]>::to_vec)),
                    Ok(AppOut::Close) | Err(TryRecvError::Disconnected) => s.close_after = true,
                    Err(TryRecvError::Empty) => {}
                }
            }
            if !s.waiting {
                if let Some(data) = s.outbox.pop_front() {
                    to_send.push(Message::new(WRTE, *id, s.host, &data));
                    s.waiting = true;
                } else if s.close_after {
                    to_send.push(Message::new(CLSE, *id, s.host, &[]));
                    s.closed = true;
                }
            }
        }
        for message in to_send {
            self.send(message);
        }
        self.streams.retain(|_, s| !s.closed);
    }

    fn handle(&mut self, message: Message) {
        {
            let mut s = self.state();
            if message.command == WRTE {
                s.seen.largest_write = s.seen.largest_write.max(message.data.len());
            }
            if message.command == AUTH {
                s.seen.auth.push(message.arg0);
            }
        }
        match message.command {
            CNXN => self.ask_for_key(),
            AUTH if message.arg0 == AUTH_SIGNATURE => {
                let mut prefixed = SHA1_PREFIX.to_vec();
                prefixed.extend_from_slice(&self.token);
                let allowed = self.state().known_keys.iter().any(|k| k.verify(Pkcs1v15Sign::new_unprefixed(), &prefixed, &message.data).is_ok());
                if allowed { self.hello() } else { self.ask_for_key() }
            }
            AUTH if message.arg0 == AUTH_RSAPUBLICKEY => {
                let text = String::from_utf8_lossy(&message.data).trim_end_matches('\0').to_string();
                let encoded = text.split(' ').next().unwrap_or_default();
                let blob = base64_decode(encoded);
                if blob.len() != 524 {
                    return;
                }
                let n = BigUint::from_bytes_le(&blob[8..264]);
                let e = BigUint::from_bytes_le(&blob[520..524]);
                let Ok(key) = RsaPublicKey::new(n, e) else { return };
                let waiting = {
                    let mut s = self.state();
                    s.seen.prompts += 1;
                    if s.person == Person::Allows {
                        s.known_keys.push(key);
                        None
                    } else {
                        Some(key)
                    }
                };
                let allows = waiting.is_none();
                self.pending_key = waiting;
                if allows {
                    self.hello();
                }
            }
            OPEN => self.open(message),
            OKAY => {
                let id = message.arg1;
                if let Some(s) = self.streams.get_mut(&id) {
                    if s.host == 0 {
                        // The PC took the app's connection.
                        s.host = message.arg0;
                        if let Some((reply, socket)) = self.pending_app.remove(&id) {
                            let _ = reply.send(Ok(socket));
                        }
                    } else {
                        s.waiting = false;
                    }
                }
            }
            WRTE => self.write(message),
            CLSE => {
                let id = message.arg1;
                if message.arg0 == 0 {
                    // The PC refused a stream the app opened.
                    self.streams.remove(&id);
                    if let Some((reply, _)) = self.pending_app.remove(&id) {
                        let _ = reply.send(Err("the PC refused the connection".to_string()));
                    }
                } else if let Some(s) = self.streams.remove(&id)
                    && !s.closed
                {
                    self.send(Message::new(CLSE, id, s.host, &[]));
                }
            }
            _ => {}
        }
    }

    fn ask_for_key(&mut self) {
        self.tokens = self.tokens.wrapping_add(1);
        self.token = vec![self.tokens; 20];
        self.send(Message::new(AUTH, AUTH_TOKEN, 0, &self.token.clone()));
    }

    fn hello(&self) {
        let (model, max) = {
            let s = self.state();
            (s.model.clone(), s.max_data)
        };
        let banner = format!("device::ro.product.name=x1sxx;ro.product.model={model};ro.product.device=x1s;features=shell_v2,cmd,stat_v2");
        self.send(Message::new(CNXN, VERSION, max, banner.as_bytes()));
    }

    fn open(&mut self, message: Message) {
        let service = String::from_utf8_lossy(&message.data).trim_end_matches('\0').to_string();
        self.state().seen.services.push(service.clone());
        self.next_id += 1;
        let id = self.next_id;
        let host = message.arg0;
        let stream = |kind: Service, outbox: Vec<Vec<u8>>, close_after: bool| Stream {
            host,
            kind,
            outbox: outbox.into(),
            waiting: false,
            close_after,
            closed: false,
        };
        let opened = if let Some(command) = service.strip_prefix("shell,v2,raw:") {
            Some(stream(Service::Shell { command: command.to_string(), input: Vec::new() }, Vec::new(), false))
        } else if let Some(size) = service.strip_prefix("exec:cmd package install -r -S ") {
            let change = self.state().change_on_install.take();
            if let Some((path, bytes)) = change {
                std::fs::write(path, bytes).expect("the file to change could be written");
            }
            size.parse().ok().map(|size| stream(Service::Install { size, bytes: Vec::new() }, Vec::new(), false))
        } else if let Some(port) = service.strip_prefix("reverse:forward:tcp:").and_then(|r| r.split(';').next()).and_then(|p| p.parse().ok()) {
            let mut s = self.state();
            if !s.reverses.contains(&port) {
                s.reverses.push(port);
            }
            Some(stream(Service::Text, vec![b"OKAY".to_vec()], true))
        } else if let Some(port) = service.strip_prefix("reverse:killforward:tcp:").and_then(|p| p.parse::<u16>().ok()) {
            self.state().reverses.retain(|p| *p != port);
            Some(stream(Service::Text, vec![b"OKAY".to_vec()], true))
        } else {
            None
        };
        match opened {
            Some(s) => {
                self.send(Message::new(OKAY, id, host, &[]));
                self.streams.insert(id, s);
            }
            None => {
                self.state().seen.refused.push(service);
                self.send(Message::new(CLSE, 0, host, &[]));
            }
        }
    }

    fn write(&mut self, message: Message) {
        let id = message.arg1;
        let Some(host) = self.streams.get(&id).map(|s| s.host) else { return };
        self.send(Message::new(OKAY, id, host, &[]));
        let state = self.state.clone();
        let Some(s) = self.streams.get_mut(&id) else { return };
        match &mut s.kind {
            Service::Shell { command, input } => {
                input.extend_from_slice(&message.data);
                let mut stdin = Vec::new();
                let mut closed = false;
                let mut at = 0;
                while at + 5 <= input.len() {
                    let length = u32::from_le_bytes([input[at + 1], input[at + 2], input[at + 3], input[at + 4]]) as usize;
                    if at + 5 + length > input.len() {
                        break;
                    }
                    match input[at] {
                        shell_id::STDIN => stdin.extend_from_slice(&input[at + 5..at + 5 + length]),
                        shell_id::CLOSE_STDIN => closed = true,
                        _ => {}
                    }
                    at += 5 + length;
                }
                if closed {
                    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                    st.seen.commands.push(command.clone());
                    let (out, err, code) = answer(&mut st, command, &stdin);
                    drop(st);
                    if !out.is_empty() {
                        s.outbox.push_back(shell_packet(shell_id::STDOUT, &out));
                    }
                    if !err.is_empty() {
                        s.outbox.push_back(shell_packet(shell_id::STDERR, &err));
                    }
                    s.outbox.push_back(shell_packet(shell_id::EXIT, &[code]));
                    s.close_after = true;
                    input.clear();
                }
            }
            Service::Install { size, bytes } => {
                bytes.extend_from_slice(&message.data);
                if bytes.len() >= *size {
                    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                    st.seen.installs += 1;
                    st.seen.installed.push(bytes.clone());
                    let said = install(&mut st, bytes);
                    s.outbox.push_back(said.into_bytes());
                    s.close_after = true;
                }
            }
            Service::App { to_app, .. } => {
                let _ = to_app.send(message.data);
            }
            Service::Text => {}
        }
    }
}

/// Reads one message from the PC, waiting at most `wait` for it to begin.
fn read_host(from_host: &Receiver<Vec<u8>>, wait: Duration) -> Result<Option<Message>, ()> {
    let header = match from_host.recv_timeout(wait) {
        Ok(h) => h,
        Err(RecvTimeoutError::Timeout) => return Ok(None),
        Err(RecvTimeoutError::Disconnected) => return Err(()),
    };
    if header.len() != 24 {
        return Err(());
    }
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    let mut bytes = header;
    while bytes.len() < 24 + length {
        bytes.extend(from_host.recv_timeout(Duration::from_secs(5)).map_err(|_| ())?);
    }
    Message::parse(&bytes).map(Some).map_err(|_| ())
}

/// The phone's answer to one of LiMo's commands: its standard output, its
/// standard error and its exit code.
fn answer(s: &mut FakeState, command: &str, stdin: &[u8]) -> (Vec<u8>, Vec<u8>, u8) {
    let out = |text: String| (text.into_bytes(), Vec::new(), 0u8);
    let ask = Ask::every().into_iter().find(|a| a.command() == command);
    match ask {
        Some(Ask::Manufacturer) => out(format!("{}\n", s.manufacturer)),
        Some(Ask::Model) => out(format!("{}\n", s.model)),
        Some(Ask::AndroidVersion) => out(format!("{}\n", s.android)),
        Some(Ask::Sdk) => out(format!("{}\n", s.sdk)),
        Some(Ask::UsbDebugging) => out(if s.debugging { "1\n" } else { "0\n" }.to_string()),
        Some(Ask::LimoListed) => out(if s.limo.is_some() { format!("package:{PACKAGE}\n") } else { String::new() }),
        Some(Ask::LimoVersion) => match &s.limo {
            Some(limo) => out(format!(
                "Packages:\n  Package [{PACKAGE}] (5f2c1a):\n    versionCode={} minSdk=26 targetSdk=36\n    versionName={}\n",
                limo.version_code, limo.version_name
            )),
            None => out(format!("Unable to find package: {PACKAGE}\n")),
        },
        Some(Ask::OfferIntoDoor) => {
            if s.limo.is_some() {
                s.offers.push(stdin.to_vec());
                out(String::new())
            } else {
                (Vec::new(), b"Error while accessing provider:ai.lucidloom.limo.child.cable\n".to_vec(), 1)
            }
        }
        Some(Ask::StartLimo) => {
            if s.limo.is_none() {
                out(format!("Starting: Intent {{ cmp={ACTIVITY} }}\nError type 3\nError: Activity class {{{ACTIVITY}}} does not exist.\n"))
            } else if s.screen_open {
                out(format!(
                    "Starting: Intent {{ cmp={ACTIVITY} }}\nWarning: Activity not started, intent has been delivered to currently running top-most instance.\n"
                ))
            } else {
                s.screen_open = true;
                out(format!("Starting: Intent {{ cmp={ACTIVITY} }}\n"))
            }
        }
        Some(Ask::DebuggingOff) => {
            s.debugging = false;
            out(String::new())
        }
        None => {
            s.seen.refused.push(command.to_string());
            (Vec::new(), b"/system/bin/sh: not found\n".to_vec(), 127)
        }
    }
}

/// The package manager's answer to an install: LiMo installs, with no
/// question; another key is refused, as Android refuses it.
fn install(s: &mut FakeState, bytes: &[u8]) -> String {
    match crate::apk::apk_facts(bytes) {
        Ok(facts) if facts.package == PACKAGE => {
            if s.limo.as_ref().is_some_and(|l| l.other_key) {
                return format!("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Package {PACKAGE} signatures do not match previously installed version; ignoring!]\n");
            }
            let code = s.limo.as_ref().map(|l| l.version_code + 1).unwrap_or(1);
            s.limo = Some(FakeLimo { version_code: code, version_name: facts.version_name.unwrap_or_default(), other_key: false });
            "Success\n".to_string()
        }
        Ok(_) => "Success\n".to_string(),
        Err(_) => "Failure [INSTALL_PARSE_FAILED_NOT_APK: Failed to parse]\n".to_string(),
    }
}
