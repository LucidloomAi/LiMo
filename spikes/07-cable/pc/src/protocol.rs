//! This program's own adb: the messages adb's host side speaks to a phone,
//! written here for the WinUSB route (`route_winusb.rs`), over any [`Wire`]
//! (WinUSB on the phone; an in memory fake phone in the tests).
//!
//! What it speaks, as AOSP's adb does (its `protocol.txt` and
//! `SERVICES.TXT`, and the shell protocol in `shell_protocol.h`):
//!
//! - **Messages:** a 24 byte header (command, two arguments, the data's
//!   length and its byte sum, and the command with every bit flipped), then
//!   the data. On USB the header is one transfer and the data the next.
//! - **Connecting:** `CNXN` with version 0x01000001, at most 1 MiB of data a
//!   message, and the features this side speaks (`shell_v2`, `cmd`). The
//!   phone answers `AUTH` with a 20 byte token; this side signs it with its
//!   own key (RSA, PKCS#1 v1.5 over a SHA-1 digest, as adb signs: the token
//!   stands in for the digest); a phone that knows the key answers `CNXN`;
//!   one that does not sends a new token, this side then sends its public
//!   key, the phone asks "Allow USB debugging?", and answers `CNXN` once the
//!   person allows it.
//! - **Streams:** `OPEN` with a service name; the phone's `OKAY` opens it,
//!   its `CLSE` refuses it. `WRTE` carries data one message at a time, each
//!   answered by `OKAY` before the next; `CLSE` ends it. adb's streams have
//!   no half close: either side's `CLSE` ends both directions.
//! - **Shell, with its exit code:** `shell,v2,raw:COMMAND`, the second shell
//!   protocol, with no terminal: packets of a 1 byte id, a 4 byte length and
//!   the data (0 standard input, 1 standard output, 2 standard error, 3 the
//!   exit code, 4 standard input closed). Standard input is sent, then
//!   closed, so a command that reads to its end ends.
//! - **Install:** `exec:cmd package install -r -S SIZE`, the APK streamed to
//!   it, Android's answer read to the stream's end, as Google's adb does.
//! - **Reverse:** `reverse:forward:tcp:P;tcp:P` and
//!   `reverse:killforward:tcp:P`. Each connection the phone then makes is a
//!   stream it opens (`OPEN tcp:P`), which this side relays to 127.0.0.1:P on
//!   this PC, for as long as the connection is held.
//!
//! Nothing else: no file sync, no root, no reboot, no other service.

#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rsa::RsaPrivateKey;
use rsa::pkcs1v15::Pkcs1v15Sign;

/// `CNXN`: connect.
pub const CNXN: u32 = u32::from_le_bytes(*b"CNXN");
/// `AUTH`: the key question.
pub const AUTH: u32 = u32::from_le_bytes(*b"AUTH");
/// `OPEN`: open a stream.
pub const OPEN: u32 = u32::from_le_bytes(*b"OPEN");
/// `OKAY`: a stream opened, or ready for more.
pub const OKAY: u32 = u32::from_le_bytes(*b"OKAY");
/// `CLSE`: a stream ended, or refused.
pub const CLSE: u32 = u32::from_le_bytes(*b"CLSE");
/// `WRTE`: data on a stream.
pub const WRTE: u32 = u32::from_le_bytes(*b"WRTE");
/// `STLS`: the phone asks for TLS (wireless debugging only).
pub const STLS: u32 = u32::from_le_bytes(*b"STLS");
/// The protocol version this side speaks: the one that skips the byte sum.
pub const VERSION: u32 = 0x0100_0001;
/// The most data one message may carry.
pub const MAX_DATA: usize = 1024 * 1024;
/// `AUTH` with a token from the phone.
pub const AUTH_TOKEN: u32 = 1;
/// `AUTH` with this side's signature of the token.
pub const AUTH_SIGNATURE: u32 = 2;
/// `AUTH` with this side's public key, which makes the phone ask.
pub const AUTH_RSAPUBLICKEY: u32 = 3;
/// This side's banner: the features it speaks, and no more.
pub const BANNER: &str = "host::features=shell_v2,cmd";
/// The DigestInfo prefix of a SHA-1 digest, which adb's signature carries
/// before the token (PKCS#1, RFC 8017 section 9.2).
pub const SHA1_PREFIX: [u8; 15] = [0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14];

/// Shell protocol packet ids.
pub mod shell_id {
    /// Standard input.
    pub const STDIN: u8 = 0;
    /// Standard output.
    pub const STDOUT: u8 = 1;
    /// Standard error.
    pub const STDERR: u8 = 2;
    /// The exit code, one byte.
    pub const EXIT: u8 = 3;
    /// Standard input closed.
    pub const CLOSE_STDIN: u8 = 4;
}

/// How long a read waits for a message to begin before looking whether to stop.
pub const HEADER_WAIT: Duration = Duration::from_secs(1);
/// How long a read waits for the rest of a message that has begun.
pub const DATA_WAIT: Duration = Duration::from_secs(10);
/// How long the phone has to answer this side's first `CNXN`.
pub const HELLO_WAIT: Duration = Duration::from_secs(10);

/// One adb message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// `CNXN`, `AUTH`, `OPEN`, `OKAY`, `CLSE`, `WRTE`, `STLS`.
    pub command: u32,
    /// The first argument.
    pub arg0: u32,
    /// The second argument.
    pub arg1: u32,
    /// The data.
    pub data: Vec<u8>,
}

impl Message {
    /// A message.
    pub fn new(command: u32, arg0: u32, arg1: u32, data: &[u8]) -> Message {
        Message { command, arg0, arg1, data: data.to_vec() }
    }

    /// Its 24 byte header.
    pub fn header(&self) -> [u8; 24] {
        let sum = self.data.iter().fold(0u32, |s, b| s.wrapping_add(u32::from(*b)));
        let mut out = [0u8; 24];
        for (i, v) in [self.command, self.arg0, self.arg1, self.data.len() as u32, sum, self.command ^ 0xffff_ffff].iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// Reads a message from its header and data, as [`next_message`] gives
    /// them. The byte sum is not checked, as adb's own does not since
    /// version 0x01000001.
    pub fn parse(bytes: &[u8]) -> Result<Message, String> {
        if bytes.len() < 24 {
            return Err("a message shorter than its header".to_string());
        }
        let word = |i: usize| u32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);
        let (command, length, magic) = (word(0), word(3) as usize, word(5));
        if magic != command ^ 0xffff_ffff {
            return Err("a message whose header does not check".to_string());
        }
        if bytes.len() != 24 + length {
            return Err("a message whose data is not as long as its header says".to_string());
        }
        Ok(Message { command, arg0: word(1), arg1: word(2), data: bytes[24..].to_vec() })
    }
}

/// The command's four letters, for what is said.
pub fn name(command: u32) -> String {
    String::from_utf8_lossy(&command.to_le_bytes()).into_owned()
}

/// The USB link to the phone's debugging interface, one transfer at a time.
pub trait Wire: Send + Sync {
    /// The IN pipe's packet size.
    fn packet(&self) -> usize;
    /// One transfer from the phone of at most `want` bytes (whole packets),
    /// waiting at most `wait`: its bytes (none for a zero length packet), or
    /// None when nothing came in time.
    fn read(&self, want: usize, wait: Duration) -> Result<Option<Vec<u8>>, String>;
    /// One transfer to the phone, ended with a zero length packet when it
    /// fills its last packet exactly.
    fn write(&self, bytes: &[u8]) -> Result<(), String>;
}

/// One transfer from the phone, as [`next_message`] asks for it.
pub type ReadTransfer<'a> = dyn FnMut(usize, Duration) -> Result<Option<Vec<u8>>, String> + 'a;

/// Reads one adb message from the phone as adb's own host side does: the 24
/// byte header in one transfer, then the data in transfers of what is left
/// of it rounded up to whole packets. A transfer then ends when the data
/// does, whether the phone sends a zero length packet after it or not (a
/// larger transfer would wait for more, and adb's flow control would stall).
/// Zero length packets between messages are passed over. None when no
/// message began within one wait.
pub fn next_message(read: &mut ReadTransfer<'_>, packet: usize) -> Result<Option<Vec<u8>>, String> {
    let header = loop {
        match read(packet, HEADER_WAIT)? {
            None => return Ok(None),
            Some(bytes) if bytes.is_empty() => continue,
            Some(bytes) => break bytes,
        }
    };
    if header.len() != 24 {
        return Err(format!("a message header of {} bytes", header.len()));
    }
    let length = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    if length > MAX_DATA {
        return Err("a message larger than adb allows".to_string());
    }
    let mut message = header;
    message.reserve(length);
    while message.len() < 24 + length {
        let rest = 24 + length - message.len();
        match read(rest.div_ceil(packet) * packet, DATA_WAIT)? {
            None => return Err("the phone stopped in the middle of a message".to_string()),
            Some(chunk) if chunk.len() > rest => return Err("more data than the message's header says".to_string()),
            Some(chunk) => message.extend_from_slice(&chunk),
        }
    }
    Ok(Some(message))
}

/// A shell protocol packet.
pub fn shell_packet(id: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + data.len());
    out.push(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// What a shell command gave.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellOutput {
    /// Its standard output.
    pub stdout: Vec<u8>,
    /// Its standard error.
    pub stderr: Vec<u8>,
    /// Its exit code, when the shell protocol carried one.
    pub exit: Option<i32>,
}

/// Takes every whole shell packet from the front of `buffer` into `out`; a
/// packet cut by the end of a message waits there for the rest.
pub fn take_shell_packets(buffer: &mut Vec<u8>, out: &mut ShellOutput) {
    loop {
        if buffer.len() < 5 {
            return;
        }
        let length = u32::from_le_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
        if buffer.len() < 5 + length {
            return;
        }
        let id = buffer[0];
        let data: Vec<u8> = buffer.drain(..5 + length).skip(5).collect();
        match id {
            shell_id::STDOUT => out.stdout.extend_from_slice(&data),
            shell_id::STDERR => out.stderr.extend_from_slice(&data),
            shell_id::EXIT => out.exit = data.first().map(|c| i32::from(*c)),
            _ => {}
        }
    }
}

/// Why a call to the phone gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbError {
    /// The connection to the phone is gone.
    Lost(String),
    /// It did not end in time.
    Timeout(Duration),
    /// The phone refused, in its own words when it gave any.
    Refused(String),
    /// The phone said something this side does not understand.
    Protocol(String),
}

impl std::fmt::Display for AdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdbError::Lost(why) => write!(f, "the connection to the phone is gone ({why})"),
            AdbError::Timeout(limit) => write!(f, "no answer within {} seconds", limit.as_secs()),
            AdbError::Refused(why) => write!(f, "the phone refused: {why}"),
            AdbError::Protocol(why) => write!(f, "the phone's answer was not understood: {why}"),
        }
    }
}

/// Why connecting failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// The phone was shown this side's key and did not allow it in time.
    NotAllowed(Duration),
    /// The phone did not answer at all.
    NoAnswer(Duration),
    /// The phone asked for TLS, which this side does not speak.
    WantsTls,
    /// The link failed.
    Lost(String),
    /// Something else, in words.
    Protocol(String),
}

/// What a stream gets from the reader.
#[derive(Debug)]
enum Event {
    Okay(u32),
    Write(Vec<u8>),
    Close,
}

/// What the connection and its streams share.
struct Inner {
    wire: Box<dyn Wire>,
    writing: Mutex<()>,
    max_data: AtomicUsize,
    streams: Mutex<HashMap<u32, Sender<Event>>>,
    next_id: AtomicU32,
    /// The ports reversed: connections the phone opens to `tcp:P` go to
    /// 127.0.0.1:P here, and no others.
    reverses: Mutex<HashMap<String, u16>>,
    lost: Mutex<Option<String>>,
    stop: AtomicBool,
}

impl Inner {
    fn send(&self, message: &Message) -> Result<(), AdbError> {
        let _one_at_a_time = self.writing.lock().map_err(|_| AdbError::Lost("a writer failed".to_string()))?;
        self.wire.write(&message.header()).map_err(AdbError::Lost)?;
        if !message.data.is_empty() {
            self.wire.write(&message.data).map_err(AdbError::Lost)?;
        }
        Ok(())
    }

    fn read(&self) -> Result<Option<Message>, String> {
        let mut read = |want: usize, wait: Duration| self.wire.read(want, wait);
        match next_message(&mut read, self.wire.packet())? {
            None => Ok(None),
            Some(bytes) => Message::parse(&bytes).map(Some),
        }
    }

    fn register(&self) -> (u32, Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let id = loop {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            if id != 0 {
                break id;
            }
        };
        if let Ok(mut streams) = self.streams.lock() {
            streams.insert(id, tx);
        }
        (id, rx)
    }

    fn unregister(&self, id: u32) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.remove(&id);
        }
    }

    fn lost_why(&self) -> String {
        self.lost.lock().ok().and_then(|l| l.clone()).unwrap_or_else(|| "the reader stopped".to_string())
    }

    fn max(&self) -> usize {
        self.max_data.load(Ordering::SeqCst).max(1)
    }
}

/// A connection to the phone, with its reader running.
pub struct Connection {
    inner: Arc<Inner>,
    reader: Option<JoinHandle<()>>,
    /// The phone's banner (`device::...;features=...`).
    pub banner: String,
    /// True when the phone speaks the second shell protocol.
    pub shell_v2: bool,
}

/// This side's public key as the phone takes it: the key in Android's own
/// format, base64, a space, a name, and a zero.
pub fn public_key_message(android_public_key_base64: &str, name: &str) -> Vec<u8> {
    let mut out = format!("{android_public_key_base64} {name}").into_bytes();
    out.push(0);
    out
}

/// adb's signature of a token: PKCS#1 v1.5 with the SHA-1 DigestInfo prefix,
/// the token standing in for the digest.
pub fn sign_token(key: &RsaPrivateKey, token: &[u8]) -> Result<Vec<u8>, String> {
    let mut prefixed = SHA1_PREFIX.to_vec();
    prefixed.extend_from_slice(token);
    key.sign(Pkcs1v15Sign::new_unprefixed(), &prefixed).map_err(|e| e.to_string())
}

/// Reads one message, waiting until `deadline`.
fn read_until(inner: &Inner, deadline: Instant) -> Result<Option<Message>, String> {
    while Instant::now() < deadline {
        if let Some(message) = inner.read()? {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

impl Connection {
    /// Connects over `wire` with `key`. `public_key` is the key's message for
    /// the phone ([`public_key_message`]); `prompt_wait` how long the person
    /// has to allow it, once `on_prompt` has been called.
    pub fn connect(
        wire: Box<dyn Wire>,
        key: &RsaPrivateKey,
        public_key: &[u8],
        prompt_wait: Duration,
        on_prompt: &mut dyn FnMut(),
    ) -> Result<Connection, ConnectError> {
        let inner = Arc::new(Inner {
            wire,
            writing: Mutex::new(()),
            max_data: AtomicUsize::new(MAX_DATA),
            streams: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
            reverses: Mutex::new(HashMap::new()),
            lost: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let lost = |e: AdbError| ConnectError::Lost(e.to_string());
        inner.send(&Message::new(CNXN, VERSION, MAX_DATA as u32, BANNER.as_bytes())).map_err(lost)?;
        let mut deadline = Instant::now() + HELLO_WAIT;
        let mut signed = false;
        let mut shown = false;
        let answer = loop {
            let message = match read_until(&inner, deadline).map_err(ConnectError::Lost)? {
                Some(message) => message,
                None if shown => return Err(ConnectError::NotAllowed(prompt_wait)),
                None => return Err(ConnectError::NoAnswer(HELLO_WAIT)),
            };
            match message.command {
                CNXN => break message,
                STLS => return Err(ConnectError::WantsTls),
                AUTH if message.arg0 == AUTH_TOKEN && !signed => {
                    let signature = sign_token(key, &message.data).map_err(ConnectError::Protocol)?;
                    inner.send(&Message::new(AUTH, AUTH_SIGNATURE, 0, &signature)).map_err(lost)?;
                    signed = true;
                }
                AUTH if message.arg0 == AUTH_TOKEN && !shown => {
                    inner.send(&Message::new(AUTH, AUTH_RSAPUBLICKEY, 0, public_key)).map_err(lost)?;
                    shown = true;
                    on_prompt();
                    deadline = Instant::now() + prompt_wait;
                }
                // A second token after the key was shown, or what is left
                // of an earlier connection: passed over.
                _ => {}
            }
        };
        let theirs = answer.arg1 as usize;
        if theirs > 0 {
            inner.max_data.store(theirs.min(MAX_DATA), Ordering::SeqCst);
        }
        let banner = String::from_utf8_lossy(&answer.data).trim_end_matches('\0').to_string();
        let shell_v2 = banner
            .split(';')
            .filter_map(|part| part.strip_prefix("features="))
            .flat_map(|features| features.split(','))
            .any(|f| f == "shell_v2");
        let reading = inner.clone();
        let reader = std::thread::Builder::new()
            .name("limo-adb-reader".to_string())
            .spawn(move || read_loop(reading))
            .map_err(|e| ConnectError::Protocol(format!("the reader could not start: {e}")))?;
        Ok(Connection { inner, reader: Some(reader), banner, shell_v2 })
    }

    /// The most data one message carries on this connection.
    pub fn max_data(&self) -> usize {
        self.inner.max()
    }

    /// Opens a stream to `service`.
    pub fn open(&self, service: &str, wait: Duration) -> Result<Stream, AdbError> {
        open_stream(&self.inner, service, wait)
    }

    /// Runs `command` in the phone's shell, `input` on its standard input,
    /// which is then closed. With the second shell protocol, its exit code
    /// and its standard error come back apart; without it, there is no exit
    /// code and no input can be given.
    pub fn shell(&self, command: &str, input: Option<&[u8]>, wait: Duration) -> Result<ShellOutput, AdbError> {
        let deadline = Instant::now() + wait;
        if !self.shell_v2 {
            if input.is_some() {
                return Err(AdbError::Refused("the phone has no second shell protocol, so standard input cannot be ended".to_string()));
            }
            let mut stream = self.open(&format!("shell:{command}"), wait)?;
            let stdout = stream.read_to_end(deadline)?;
            return Ok(ShellOutput { stdout, stderr: Vec::new(), exit: None });
        }
        let mut stream = self.open(&format!("shell,v2,raw:{command}"), wait)?;
        if let Some(input) = input {
            let chunk = (self.max_data() - 5).min(16 * 1024);
            for part in input.chunks(chunk) {
                stream.write(&shell_packet(shell_id::STDIN, part), deadline)?;
            }
        }
        stream.write(&shell_packet(shell_id::CLOSE_STDIN, &[]), deadline)?;
        let mut buffer = Vec::new();
        let mut out = ShellOutput::default();
        while let Some(data) = stream.read(deadline)? {
            buffer.extend_from_slice(&data);
            take_shell_packets(&mut buffer, &mut out);
        }
        Ok(out)
    }

    /// Runs `command` as `exec:` (no shell protocol), its whole answer.
    pub fn exec(&self, command: &str, wait: Duration) -> Result<String, AdbError> {
        let deadline = Instant::now() + wait;
        let mut stream = self.open(&format!("exec:{command}"), wait)?;
        Ok(String::from_utf8_lossy(&stream.read_to_end(deadline)?).into_owned())
    }

    /// Installs the APK at `apk`, replacing the app if it is there, as
    /// Google's adb streams it; Android's answer.
    pub fn install(&self, apk: &Path, wait: Duration) -> Result<String, AdbError> {
        let deadline = Instant::now() + wait;
        let mut file = std::fs::File::open(apk).map_err(|e| AdbError::Refused(format!("the APK could not be read: {e}")))?;
        let size = file.metadata().map_err(|e| AdbError::Refused(e.to_string()))?.len();
        let mut stream = self.open(&format!("exec:cmd package install -r -S {size}"), wait)?;
        let mut buffer = vec![0u8; self.max_data()];
        loop {
            let n = file.read(&mut buffer).map_err(|e| AdbError::Refused(format!("the APK could not be read: {e}")))?;
            if n == 0 {
                break;
            }
            stream.write(&buffer[..n], deadline)?;
        }
        Ok(String::from_utf8_lossy(&stream.read_to_end(deadline)?).trim().to_string())
    }

    /// Carries the phone's connections to `port` on its own loopback to
    /// `port` on this PC's.
    pub fn reverse(&self, port: u16, wait: Duration) -> Result<(), AdbError> {
        let spec = format!("tcp:{port}");
        if let Ok(mut reverses) = self.inner.reverses.lock() {
            reverses.insert(spec.clone(), port);
        }
        let result = self.reverse_request(&format!("reverse:forward:{spec};{spec}"), wait);
        if result.is_err()
            && let Ok(mut reverses) = self.inner.reverses.lock()
        {
            reverses.remove(&spec);
        }
        result
    }

    /// Removes that again.
    pub fn reverse_remove(&self, port: u16, wait: Duration) -> Result<(), AdbError> {
        let spec = format!("tcp:{port}");
        self.reverse_request(&format!("reverse:killforward:{spec}"), wait)?;
        if let Ok(mut reverses) = self.inner.reverses.lock() {
            reverses.remove(&spec);
        }
        Ok(())
    }

    fn reverse_request(&self, service: &str, wait: Duration) -> Result<(), AdbError> {
        let deadline = Instant::now() + wait;
        let mut stream = self.open(service, wait)?;
        let answer = stream.read_to_end(deadline)?;
        match answer.get(..4) {
            Some(b"OKAY") => Ok(()),
            Some(b"FAIL") => Err(AdbError::Refused(String::from_utf8_lossy(answer.get(8..).unwrap_or_default()).into_owned())),
            _ => Err(AdbError::Protocol(format!("an answer to {service} of {} bytes", answer.len()))),
        }
    }

    /// Ends the connection: the reader stops, the streams end.
    pub fn close(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            let deadline = Instant::now() + HEADER_WAIT * 3;
            while !reader.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if reader.is_finished() {
                let _ = reader.join();
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reads every message and hands each to its stream, until stopped or the
/// link fails.
fn read_loop(inner: Arc<Inner>) {
    while !inner.stop.load(Ordering::SeqCst) {
        match inner.read() {
            Ok(None) => continue,
            Ok(Some(message)) => {
                if let Err(why) = dispatch(&inner, message) {
                    log::debug!("adb reader: {why}");
                }
            }
            Err(why) => {
                if let Ok(mut lost) = inner.lost.lock() {
                    *lost = Some(why);
                }
                break;
            }
        }
    }
    // Every stream learns that the link is gone.
    if let Ok(mut streams) = inner.streams.lock() {
        streams.clear();
    }
}

fn dispatch(inner: &Arc<Inner>, message: Message) -> Result<(), String> {
    let to = |id: u32, event: Event| {
        let sender = inner.streams.lock().ok().and_then(|s| s.get(&id).cloned());
        match sender {
            Some(sender) => sender.send(event).is_ok(),
            None => false,
        }
    };
    match message.command {
        OKAY => {
            to(message.arg1, Event::Okay(message.arg0));
        }
        WRTE => {
            if !to(message.arg1, Event::Write(message.data)) {
                // Data for a stream this side does not hold: end it.
                inner.send(&Message::new(CLSE, 0, message.arg0, &[])).map_err(|e| e.to_string())?;
            }
        }
        CLSE => {
            to(message.arg1, Event::Close);
            inner.unregister(message.arg1);
        }
        OPEN => incoming(inner, message.arg0, &message.data)?,
        CNXN => {
            if let Ok(mut lost) = inner.lost.lock() {
                *lost = Some("the phone started a new connection".to_string());
            }
            inner.stop.store(true, Ordering::SeqCst);
        }
        _ => {}
    }
    Ok(())
}

/// A stream the phone opens: a connection to a reversed port, relayed to
/// 127.0.0.1 here; refused for any port not reversed.
fn incoming(inner: &Arc<Inner>, remote: u32, destination: &[u8]) -> Result<(), String> {
    let destination = String::from_utf8_lossy(destination).trim_end_matches('\0').to_string();
    let port = inner.reverses.lock().ok().and_then(|r| r.get(&destination).copied());
    let Some(port) = port else {
        return inner.send(&Message::new(CLSE, 0, remote, &[])).map_err(|e| e.to_string());
    };
    let relaying = inner.clone();
    std::thread::Builder::new()
        .name("limo-adb-reverse".to_string())
        .spawn(move || relay(relaying, remote, port))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Relays one connection the phone opened, to 127.0.0.1:`port`, until either
/// side ends it.
fn relay(inner: Arc<Inner>, remote: u32, port: u16) {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(tcp) = TcpStream::connect_timeout(&address, Duration::from_secs(5)) else {
        let _ = inner.send(&Message::new(CLSE, 0, remote, &[]));
        return;
    };
    let _ = tcp.set_nodelay(true);
    let (local, events) = inner.register();
    if inner.send(&Message::new(OKAY, local, remote, &[])).is_err() {
        inner.unregister(local);
        return;
    }
    let Ok(mut from_pc) = tcp.try_clone() else {
        let _ = inner.send(&Message::new(CLSE, local, remote, &[]));
        inner.unregister(local);
        return;
    };
    let (acks, acked) = mpsc::channel::<()>();
    let sending = inner.clone();
    let max = inner.max();
    let up = std::thread::spawn(move || {
        let mut buffer = vec![0u8; max];
        'reading: loop {
            let n = match from_pc.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for part in buffer[..n].chunks(max) {
                if sending.send(&Message::new(WRTE, local, remote, part)).is_err() {
                    break 'reading;
                }
                if acked.recv_timeout(Duration::from_secs(30)).is_err() {
                    break 'reading;
                }
            }
        }
        let _ = sending.send(&Message::new(CLSE, local, remote, &[]));
    });
    let mut to_pc = tcp;
    loop {
        match events.recv_timeout(HEADER_WAIT) {
            Ok(Event::Okay(_)) => {
                let _ = acks.send(());
            }
            Ok(Event::Write(data)) => {
                if to_pc.write_all(&data).is_err() {
                    break;
                }
                if inner.send(&Message::new(OKAY, local, remote, &[])).is_err() {
                    break;
                }
            }
            Ok(Event::Close) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if inner.stop.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }
    let _ = to_pc.shutdown(Shutdown::Both);
    drop(acks);
    inner.unregister(local);
    let _ = up.join();
}

fn open_stream(inner: &Arc<Inner>, service: &str, wait: Duration) -> Result<Stream, AdbError> {
    let (local, events) = inner.register();
    let mut name = service.as_bytes().to_vec();
    name.push(0);
    if let Err(e) = inner.send(&Message::new(OPEN, local, 0, &name)) {
        inner.unregister(local);
        return Err(e);
    }
    let answer = events.recv_timeout(wait);
    match answer {
        Ok(Event::Okay(remote)) => Ok(Stream { inner: inner.clone(), local, remote, events, pending: VecDeque::new(), closed: false }),
        Ok(Event::Close) => {
            inner.unregister(local);
            Err(AdbError::Refused(format!("the phone has no service {service}")))
        }
        Ok(Event::Write(_)) => {
            inner.unregister(local);
            Err(AdbError::Protocol("data before the stream opened".to_string()))
        }
        Err(RecvTimeoutError::Timeout) => {
            inner.unregister(local);
            Err(AdbError::Timeout(wait))
        }
        Err(RecvTimeoutError::Disconnected) => Err(AdbError::Lost(inner.lost_why())),
    }
}

/// A stream this side opened.
pub struct Stream {
    inner: Arc<Inner>,
    local: u32,
    remote: u32,
    events: Receiver<Event>,
    pending: VecDeque<Vec<u8>>,
    closed: bool,
}

impl Stream {
    fn next_event(&self, deadline: Instant) -> Result<Event, AdbError> {
        let wait = deadline.saturating_duration_since(Instant::now());
        match self.events.recv_timeout(wait) {
            Ok(event) => Ok(event),
            Err(RecvTimeoutError::Timeout) => Err(AdbError::Timeout(wait)),
            Err(RecvTimeoutError::Disconnected) => Err(AdbError::Lost(self.inner.lost_why())),
        }
    }

    /// Sends `data`, a message at a time, each one acknowledged before the
    /// next; data that comes meanwhile is kept for [`Stream::read`].
    pub fn write(&mut self, data: &[u8], deadline: Instant) -> Result<(), AdbError> {
        for part in data.chunks(self.inner.max()) {
            if self.closed {
                return Err(AdbError::Lost("the phone ended the stream".to_string()));
            }
            self.inner.send(&Message::new(WRTE, self.local, self.remote, part))?;
            loop {
                match self.next_event(deadline)? {
                    Event::Okay(_) => break,
                    Event::Write(received) => {
                        self.pending.push_back(received);
                        self.inner.send(&Message::new(OKAY, self.local, self.remote, &[]))?;
                    }
                    Event::Close => {
                        self.closed = true;
                        return Err(AdbError::Lost("the phone ended the stream while it was being written".to_string()));
                    }
                }
            }
        }
        Ok(())
    }

    /// The next data, or None once the phone has ended the stream.
    pub fn read(&mut self, deadline: Instant) -> Result<Option<Vec<u8>>, AdbError> {
        if let Some(data) = self.pending.pop_front() {
            return Ok(Some(data));
        }
        while !self.closed {
            match self.next_event(deadline)? {
                Event::Write(data) => {
                    self.inner.send(&Message::new(OKAY, self.local, self.remote, &[]))?;
                    return Ok(Some(data));
                }
                Event::Okay(_) => {}
                Event::Close => self.closed = true,
            }
        }
        Ok(None)
    }

    /// Everything until the phone ends the stream.
    pub fn read_to_end(&mut self, deadline: Instant) -> Result<Vec<u8>, AdbError> {
        let mut all = Vec::new();
        while let Some(data) = self.read(deadline)? {
            all.extend_from_slice(&data);
        }
        Ok(all)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.inner.send(&Message::new(CLSE, self.local, self.remote, &[]));
        }
        self.inner.unregister(self.local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_holds_the_length_the_sum_and_the_flipped_command() {
        let m = Message::new(WRTE, 7, 9, b"abc");
        let h = m.header();
        assert_eq!(&h[0..4], b"WRTE");
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(h[12..16].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(h[16..20].try_into().unwrap()), 97 + 98 + 99);
        assert_eq!(u32::from_le_bytes(h[20..24].try_into().unwrap()), WRTE ^ 0xffff_ffff);
        let mut bytes = h.to_vec();
        bytes.extend_from_slice(b"abc");
        assert_eq!(Message::parse(&bytes).unwrap(), m);
        bytes[20] ^= 1;
        assert!(Message::parse(&bytes).unwrap_err().contains("does not check"));
        assert!(Message::parse(&h[..20]).is_err());
        assert_eq!(name(CNXN), "CNXN");
    }

    #[test]
    fn shell_packets_are_taken_whole_even_when_cut_across_messages() {
        let mut all = shell_packet(shell_id::STDOUT, b"SM-G988B\n");
        all.extend(shell_packet(shell_id::STDERR, b"warning\n"));
        all.extend(shell_packet(shell_id::EXIT, &[3]));
        let mut out = ShellOutput::default();
        let mut buffer = Vec::new();
        for part in all.chunks(4) {
            buffer.extend_from_slice(part);
            take_shell_packets(&mut buffer, &mut out);
        }
        assert!(buffer.is_empty());
        assert_eq!(out, ShellOutput { stdout: b"SM-G988B\n".to_vec(), stderr: b"warning\n".to_vec(), exit: Some(3) });
        assert_eq!(shell_packet(shell_id::CLOSE_STDIN, &[]), vec![4, 0, 0, 0, 0]);
    }

    /// The phone's transfers, given in advance, and what was asked.
    struct FakeIn {
        transfers: VecDeque<Option<Vec<u8>>>,
        asked: Vec<usize>,
    }

    impl FakeIn {
        fn new(transfers: Vec<Option<Vec<u8>>>) -> FakeIn {
            FakeIn { transfers: transfers.into(), asked: Vec::new() }
        }

        fn message(&mut self) -> Result<Option<Vec<u8>>, String> {
            let mut read = |want: usize, _wait: Duration| {
                self.asked.push(want);
                Ok(self.transfers.pop_front().unwrap_or(None))
            };
            next_message(&mut read, 512)
        }
    }

    fn header(length: u32) -> Vec<u8> {
        let mut h = vec![0u8; 24];
        h[12..16].copy_from_slice(&length.to_le_bytes());
        h
    }

    #[test]
    fn a_message_from_the_phone_is_read_in_transfers_that_end_where_it_does() {
        // 1024 bytes of data: exactly two packets of 512, with no zero length
        // packet after them, read in a transfer of exactly 1024.
        let mut phone = FakeIn::new(vec![Some(header(1024)), Some(vec![7u8; 1024])]);
        assert_eq!(phone.message().unwrap().unwrap().len(), 24 + 1024);
        assert_eq!(phone.asked, vec![512, 1024]);
        // 700 bytes arriving as 512 and then 188, after a zero length packet:
        // the next ask is for what is left, rounded up to a whole packet.
        let mut phone = FakeIn::new(vec![Some(vec![]), Some(header(700)), Some(vec![1u8; 512]), Some(vec![2u8; 188])]);
        assert_eq!(phone.message().unwrap().unwrap().len(), 724);
        assert_eq!(phone.asked, vec![512, 512, 1024, 512]);
        let mut phone = FakeIn::new(vec![Some(header(0))]);
        assert_eq!(phone.message().unwrap().unwrap(), header(0));
    }

    #[test]
    fn nothing_yet_is_none_and_a_broken_message_is_an_error() {
        assert_eq!(FakeIn::new(vec![None]).message().unwrap(), None);
        assert!(FakeIn::new(vec![Some(vec![0u8; 30])]).message().unwrap_err().contains("header of 30 bytes"));
        assert!(FakeIn::new(vec![Some(header(100)), None]).message().unwrap_err().contains("middle of a message"));
        assert!(FakeIn::new(vec![Some(header(100)), Some(vec![0u8; 200])]).message().unwrap_err().contains("more data"));
        assert!(FakeIn::new(vec![Some(header(MAX_DATA as u32 + 1))]).message().unwrap_err().contains("larger than adb allows"));
    }
}
