//! A fake phone at the far end of the USB cable: an adbd that speaks the
//! protocol as AOSP's does (as `src/protocol.rs` describes it), over an in
//! memory wire with USB's shape (one transfer at a time, the header and the
//! data apart). It asks for the key, checks the signature against the keys
//! it allows, shows the key and lets the "person" allow it or not, opens
//! the services this program uses and refuses every other, answers a few
//! shell commands, takes standard input and installs, keeps reverses, and
//! plays an app that connects to a reversed port. Everything it is asked is
//! recorded for the tests.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use limo_cable_spike::protocol::{
    AUTH, AUTH_RSAPUBLICKEY, AUTH_SIGNATURE, AUTH_TOKEN, CLSE, CNXN, Message, OKAY, OPEN, SHA1_PREFIX, VERSION, WRTE,
    Wire, shell_id, shell_packet,
};
use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::{BigUint, RsaPublicKey};

/// The packet size of the fake USB link.
pub const PACKET: usize = 512;

/// This side of the wire: what the program under test holds.
pub struct HostWire {
    to_phone: Mutex<Sender<Vec<u8>>>,
    from_phone: Mutex<(Receiver<Vec<u8>>, VecDeque<u8>)>,
}

impl Wire for HostWire {
    fn packet(&self) -> usize {
        PACKET
    }

    fn read(&self, want: usize, wait: Duration) -> Result<Option<Vec<u8>>, String> {
        let mut guard = self.from_phone.lock().unwrap();
        let (rx, rest) = &mut *guard;
        if rest.is_empty() {
            match rx.recv_timeout(wait) {
                Ok(transfer) => {
                    if transfer.is_empty() {
                        return Ok(Some(Vec::new()));
                    }
                    rest.extend(transfer);
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => return Err("the fake phone is gone".to_string()),
            }
        }
        let n = want.min(rest.len());
        Ok(Some(rest.drain(..n).collect()))
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.to_phone.lock().unwrap().send(bytes.to_vec()).map_err(|_| "the fake phone is gone".to_string())
    }
}

/// How the fake phone behaves, and what it remembers between connections.
#[derive(Debug)]
pub struct Phone {
    /// The keys it allows without asking.
    pub known: Vec<RsaPublicKey>,
    /// Whether the person taps Allow when asked.
    pub person_allows: bool,
    /// The most data one of its messages carries.
    pub max_data: u32,
    /// Its features.
    pub features: String,
    /// What it was asked, for the tests.
    pub seen: Seen,
    /// The test app's state, for the commands that ask about it.
    pub installed: bool,
    /// USB debugging, as `settings` reads it.
    pub adb_enabled: bool,
}

impl Default for Phone {
    fn default() -> Self {
        Phone {
            known: Vec::new(),
            person_allows: true,
            max_data: 4096,
            features: "shell_v2,cmd,stat_v2".to_string(),
            seen: Seen::default(),
            installed: false,
            adb_enabled: true,
        }
    }
}

/// What the fake phone was asked.
#[derive(Debug, Default, Clone)]
pub struct Seen {
    /// Every service opened, in order, refused ones too.
    pub services: Vec<String>,
    /// Services refused.
    pub refused: Vec<String>,
    /// How many times it showed "Allow USB debugging?".
    pub prompts: u32,
    /// The keys it was shown.
    pub keys_shown: Vec<RsaPublicKey>,
    /// The names given with them.
    pub key_names: Vec<String>,
    /// The bytes of the last command's standard input.
    pub stdin: Vec<u8>,
    /// Whether standard input was closed after it.
    pub stdin_closed: bool,
    /// The bytes of the last install.
    pub installed_bytes: Vec<u8>,
    /// The largest data in one message from the program.
    pub largest_write: usize,
    /// What the app got back through the reverse.
    pub app_received: Vec<u8>,
    /// Streams the app opened that the program refused.
    pub app_refused: u32,
    /// The program's banner.
    pub banner: String,
}

/// What the test tells the fake phone to do.
pub enum Control {
    /// The app connects to 127.0.0.1:`port` on the phone and sends `bytes`.
    AppConnects { port: u16, bytes: usize },
    /// The phone goes (unplugged, debugging off).
    Unplug,
}

enum Kind {
    Shell { command: String, input: Vec<u8> },
    Install { size: usize },
    Text,
    App { want: usize },
}

struct DevStream {
    host: u32,
    kind: Kind,
    outbox: VecDeque<Vec<u8>>,
    waiting: bool,
    close_after: bool,
    closed: bool,
}

/// The app's bytes: a pattern, as the real test app's.
pub fn pattern(bytes: usize) -> Vec<u8> {
    (0..bytes).map(|i| (i % 253) as u8).collect()
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

/// A running fake phone.
pub struct FakeAdbd {
    pub phone: Arc<Mutex<Phone>>,
    pub control: Sender<Control>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeAdbd {
    /// A fake phone and the wire to it.
    pub fn start(phone: Arc<Mutex<Phone>>) -> (FakeAdbd, HostWire) {
        let (to_phone, from_host) = mpsc::channel();
        let (to_host, from_phone) = mpsc::channel();
        let (control, commands) = mpsc::channel();
        let wire = HostWire { to_phone: Mutex::new(to_phone), from_phone: Mutex::new((from_phone, VecDeque::new())) };
        let state = phone.clone();
        let thread = std::thread::spawn(move || run(state, from_host, to_host, commands));
        (FakeAdbd { phone, control, thread: Some(thread) }, wire)
    }

    /// What it has seen so far.
    pub fn seen(&self) -> Seen {
        self.phone.lock().unwrap().seen.clone()
    }
}

impl Drop for FakeAdbd {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Unplug);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn send(to_host: &Sender<Vec<u8>>, message: Message) -> bool {
    // As adbd on USB: the header in one transfer, the data in the next.
    if to_host.send(message.header().to_vec()).is_err() {
        return false;
    }
    message.data.is_empty() || to_host.send(message.data).is_ok()
}

/// Reads one message from the program, waiting at most `wait`.
fn read_host(from_host: &Receiver<Vec<u8>>, wait: Duration) -> Result<Option<Message>, ()> {
    let header = match from_host.recv_timeout(wait) {
        Ok(h) => h,
        Err(RecvTimeoutError::Timeout) => return Ok(None),
        Err(RecvTimeoutError::Disconnected) => return Err(()),
    };
    assert_eq!(header.len(), 24, "the program sends each header as a transfer of its own");
    let length = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
    let mut bytes = header;
    while bytes.len() < 24 + length {
        bytes.extend(from_host.recv_timeout(Duration::from_secs(5)).map_err(|_| ())?);
    }
    Ok(Some(Message::parse(&bytes).expect("the program's message checks")))
}

fn answer(phone: &Phone, command: &str) -> (Vec<u8>, Vec<u8>, u8) {
    match command {
        "getprop ro.product.model" => (b"SM-G988B\n".to_vec(), Vec::new(), 0),
        "getprop ro.build.version.release" => (b"13\n".to_vec(), Vec::new(), 0),
        "getprop ro.build.version.sdk" => (b"33\n".to_vec(), Vec::new(), 0),
        "settings get global adb_enabled" => (if phone.adb_enabled { b"1\n".to_vec() } else { b"0\n".to_vec() }, Vec::new(), 0),
        "pm list packages ai.lucidloom.limo.spike07" => {
            (if phone.installed { b"package:ai.lucidloom.limo.spike07\n".to_vec() } else { Vec::new() }, Vec::new(), 0)
        }
        "false" => (Vec::new(), b"nope\n".to_vec(), 1),
        _ => (Vec::new(), Vec::new(), 0),
    }
}

fn run(phone: Arc<Mutex<Phone>>, from_host: Receiver<Vec<u8>>, to_host: Sender<Vec<u8>>, commands: Receiver<Control>) {
    let mut streams: HashMap<u32, DevStream> = HashMap::new();
    let mut next_id = 1000u32;
    let mut token = vec![0u8; 20];
    let mut tokens = 0u8;
    loop {
        // What the test asks.
        match commands.try_recv() {
            Ok(Control::Unplug) | Err(mpsc::TryRecvError::Disconnected) => return,
            Ok(Control::AppConnects { port, bytes }) => {
                next_id += 1;
                let max = phone.lock().unwrap().max_data as usize;
                let outbox = pattern(bytes).chunks(max).map(<[u8]>::to_vec).collect();
                let mut name = format!("tcp:{port}").into_bytes();
                name.push(0);
                send(&to_host, Message::new(OPEN, next_id, 0, &name));
                streams.insert(next_id, DevStream { host: 0, kind: Kind::App { want: bytes }, outbox, waiting: false, close_after: false, closed: false });
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        // What is ready to go.
        for (id, s) in streams.iter_mut() {
            if s.host == 0 || s.closed {
                continue;
            }
            if !s.waiting {
                if let Some(data) = s.outbox.pop_front() {
                    send(&to_host, Message::new(WRTE, *id, s.host, &data));
                    s.waiting = true;
                } else if s.close_after {
                    send(&to_host, Message::new(CLSE, *id, s.host, &[]));
                    s.closed = true;
                }
            }
        }
        let message = match read_host(&from_host, Duration::from_millis(20)) {
            Ok(Some(m)) => m,
            Ok(None) => continue,
            Err(()) => return,
        };
        let mut p = phone.lock().unwrap();
        p.seen.largest_write = p.seen.largest_write.max(if message.command == WRTE { message.data.len() } else { 0 });
        match message.command {
            CNXN => {
                p.seen.banner = String::from_utf8_lossy(&message.data).into_owned();
                tokens += 1;
                token = vec![tokens; 20];
                send(&to_host, Message::new(AUTH, AUTH_TOKEN, 0, &token));
            }
            AUTH if message.arg0 == AUTH_SIGNATURE => {
                let mut prefixed = SHA1_PREFIX.to_vec();
                prefixed.extend_from_slice(&token);
                let allowed = p.known.iter().any(|k| k.verify(Pkcs1v15Sign::new_unprefixed(), &prefixed, &message.data).is_ok());
                if allowed {
                    let banner = format!("device::ro.product.name=x1sxx;ro.product.model=SM-G988B;ro.product.device=x1s;features={}", p.features);
                    send(&to_host, Message::new(CNXN, VERSION, p.max_data, banner.as_bytes()));
                } else {
                    tokens += 1;
                    token = vec![tokens; 20];
                    send(&to_host, Message::new(AUTH, AUTH_TOKEN, 0, &token));
                }
            }
            AUTH if message.arg0 == AUTH_RSAPUBLICKEY => {
                let text = String::from_utf8_lossy(&message.data).trim_end_matches('\0').to_string();
                let (encoded, name) = text.split_once(' ').unwrap_or((&text, ""));
                let blob = base64_decode(encoded);
                assert_eq!(blob.len(), 524, "Android's public key format");
                let n = BigUint::from_bytes_le(&blob[8..264]);
                let e = BigUint::from_bytes_le(&blob[520..524]);
                let key = RsaPublicKey::new(n, e).expect("a key Android can read");
                p.seen.prompts += 1;
                p.seen.keys_shown.push(key.clone());
                p.seen.key_names.push(name.to_string());
                if p.person_allows {
                    p.known.push(key);
                    let banner = format!("device::ro.product.model=SM-G988B;features={}", p.features);
                    send(&to_host, Message::new(CNXN, VERSION, p.max_data, banner.as_bytes()));
                }
            }
            OPEN => {
                let service = String::from_utf8_lossy(&message.data).trim_end_matches('\0').to_string();
                p.seen.services.push(service.clone());
                next_id += 1;
                let id = next_id;
                let host = message.arg0;
                let stream = |kind: Kind, outbox: Vec<Vec<u8>>, close_after: bool| DevStream {
                    host,
                    kind,
                    outbox: outbox.into(),
                    waiting: false,
                    close_after,
                    closed: false,
                };
                let opened = if let Some(command) = service.strip_prefix("shell,v2,raw:") {
                    Some(stream(Kind::Shell { command: command.to_string(), input: Vec::new() }, Vec::new(), false))
                } else if let Some(size) = service.strip_prefix("exec:cmd package install -r -S ") {
                    Some(stream(Kind::Install { size: size.parse().unwrap() }, Vec::new(), false))
                } else if service == "exec:cmd package uninstall ai.lucidloom.limo.spike07" {
                    let words = if p.installed { "Success\n" } else { "Failure [DELETE_FAILED_INTERNAL_ERROR]\n" };
                    p.installed = false;
                    Some(stream(Kind::Text, vec![words.as_bytes().to_vec()], true))
                } else if service.starts_with("reverse:forward:tcp:") || service.starts_with("reverse:killforward:tcp:") {
                    Some(stream(Kind::Text, vec![b"OKAY".to_vec()], true))
                } else {
                    None
                };
                match opened {
                    Some(s) => {
                        send(&to_host, Message::new(OKAY, id, host, &[]));
                        streams.insert(id, s);
                    }
                    None => {
                        p.seen.refused.push(service);
                        send(&to_host, Message::new(CLSE, 0, host, &[]));
                    }
                }
            }
            OKAY => {
                if let Some(s) = streams.get_mut(&message.arg1) {
                    if s.host == 0 {
                        s.host = message.arg0;
                    }
                    s.waiting = false;
                }
            }
            WRTE => {
                let Some(s) = streams.get_mut(&message.arg1) else { continue };
                assert!(message.data.len() <= p.max_data as usize, "never more than the phone takes in one message");
                send(&to_host, Message::new(OKAY, message.arg1, s.host, &[]));
                match &mut s.kind {
                    Kind::Shell { command, input } => {
                        input.extend_from_slice(&message.data);
                        // Whole packets only.
                        let mut stdin = Vec::new();
                        let mut closed = false;
                        let mut at = 0;
                        while at + 5 <= input.len() {
                            let length = u32::from_le_bytes(input[at + 1..at + 5].try_into().unwrap()) as usize;
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
                            p.seen.stdin = stdin;
                            p.seen.stdin_closed = true;
                            let (out, err, code) = if command == "settings put global adb_enabled 0" {
                                p.adb_enabled = false;
                                (Vec::new(), Vec::new(), 0)
                            } else {
                                answer(&p, command)
                            };
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
                    Kind::Install { size } => {
                        p.seen.installed_bytes.extend_from_slice(&message.data);
                        if p.seen.installed_bytes.len() >= *size {
                            p.installed = true;
                            s.outbox.push_back(b"Success\n".to_vec());
                            s.close_after = true;
                        }
                    }
                    Kind::App { want } => {
                        p.seen.app_received.extend_from_slice(&message.data);
                        if p.seen.app_received.len() >= *want {
                            s.close_after = true;
                        }
                    }
                    Kind::Text => {}
                }
            }
            CLSE => {
                if message.arg0 == 0 {
                    // The program refused a stream the app opened.
                    if streams.remove(&message.arg1).is_some() {
                        p.seen.app_refused += 1;
                    }
                } else if let Some(s) = streams.remove(&message.arg1)
                    && !s.closed
                {
                    send(&to_host, Message::new(CLSE, message.arg1, s.host, &[]));
                }
            }
            _ => {}
        }
        if !p.adb_enabled {
            // USB debugging switched off: the interface goes.
            return;
        }
    }
}
