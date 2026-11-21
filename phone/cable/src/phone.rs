//! A phone connected over the cable, and every call LiMo makes on it.
//!
//! [`Phone::connect`] speaks adb's handshake over a [`Wire`] with LiMo's own
//! key; once the phone answers, the calls are LiMo's alone: what the phone
//! is, whether LiMo is on it, install LiMo (replacing it), hand LiMo its
//! pairing offer through its guarded door, carry the link's port with
//! `reverse`, start LiMo, and, as its own call, switch USB debugging off,
//! which ends the link. Every call has a time limit ([`Waits`]).

#![forbid(unsafe_code)]

use std::path::Path;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::apk::{ApkProblem, apk_facts};
use crate::commands::{self, Ask, Installed, LINK_PORT, MAX_OFFER_BYTES, OLDEST_SDK, PACKAGE, Started};
use crate::key::AdbKey;
use crate::protocol::{AdbError, ConnectError, Connection, Wire};

/// How long each kind of call may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waits {
    /// How long the phone has to answer at all.
    pub hello: Duration,
    /// How long the person has to answer "Allow USB debugging?".
    pub prompt: Duration,
    /// How long an ordinary call may take.
    pub call: Duration,
    /// How long an install may take.
    pub install: Duration,
}

impl Default for Waits {
    /// Ten seconds for the phone to answer, two minutes for the person (the
    /// guardian may need to find the phone, unlock it and read the
    /// question), half a minute a call, three minutes an install.
    fn default() -> Self {
        Waits {
            hello: Duration::from_secs(10),
            prompt: Duration::from_secs(120),
            call: Duration::from_secs(30),
            install: Duration::from_secs(180),
        }
    }
}

/// Why the phone could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenProblem {
    /// No phone that LiMo can open is plugged in.
    NoPhone,
    /// More than one is.
    TwoPhones(usize),
    /// Google's adb server runs on this PC, and holds the phone.
    AdbServerRunning,
    /// Another program holds the phone's debugging interface.
    Busy,
    /// The interface's driver is not one LiMo can open; Windows' words.
    DriverNotUsable(String),
    /// The phone was shown LiMo's key and the person did not allow it in time.
    NotAllowed,
    /// The interface opened, but adb on the phone did not answer.
    NoAnswer,
    /// The link failed while connecting.
    Lost(String),
    /// Anything else, in words.
    Other(String),
}

impl std::fmt::Display for OpenProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenProblem::NoPhone => f.write_str("no phone that LiMo can open is plugged in"),
            OpenProblem::TwoPhones(n) => write!(f, "{n} phones are plugged in"),
            OpenProblem::AdbServerRunning => f.write_str("Google's adb server runs on this PC and holds the phone"),
            OpenProblem::Busy => f.write_str("another program holds the phone's debugging interface"),
            OpenProblem::DriverNotUsable(why) => write!(f, "the phone's driver is not one LiMo can open ({why})"),
            OpenProblem::NotAllowed => f.write_str("the phone did not allow this PC in time"),
            OpenProblem::NoAnswer => f.write_str("the phone's debugging interface opened, but the phone did not answer"),
            OpenProblem::Lost(why) => write!(f, "the link to the phone failed while connecting ({why})"),
            OpenProblem::Other(why) => f.write_str(why),
        }
    }
}

/// Why a call gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// The link to the phone is gone (unplugged, or USB debugging off).
    Lost(String),
    /// It did not end in time.
    Timeout(Duration),
    /// The phone refused, or answered what LiMo does not understand.
    Failed(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Lost(why) => write!(f, "the link to the phone is gone ({why})"),
            CallError::Timeout(limit) => write!(f, "the phone gave no answer within {} seconds", limit.as_secs()),
            CallError::Failed(why) => f.write_str(why),
        }
    }
}

fn call_error(error: AdbError) -> CallError {
    match error {
        AdbError::Lost(why) => CallError::Lost(why),
        AdbError::Timeout(limit) => CallError::Timeout(limit),
        other => CallError::Failed(other.to_string()),
    }
}

/// What the phone is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhoneFacts {
    /// Its maker, as Android names it ("samsung").
    pub manufacturer: String,
    /// Its model ("SM-G988B").
    pub model: String,
    /// Its Android version ("13").
    pub android: String,
    /// Its API level, when it could be read.
    pub sdk: Option<u32>,
    /// Whether USB debugging is on, as the phone's setting reads.
    pub usb_debugging: Option<bool>,
}

impl PhoneFacts {
    /// True when its Android is older than the oldest LiMo runs on.
    pub fn too_old(&self) -> bool {
        self.sdk.is_some_and(|sdk| sdk < OLDEST_SDK)
    }
}

/// LiMo's child form on the phone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LimoOnPhone {
    /// Its version code, when it could be read.
    pub version_code: Option<u64>,
    /// Its version name, when it could be read.
    pub version_name: Option<String>,
}

/// Why LiMo was not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The APK could not be read as one: nothing was sent.
    NotAnApk(ApkProblem),
    /// The APK is another app: nothing was sent.
    NotLimo(String),
    /// The APK is a debug build: nothing was sent.
    DebugBuild,
    /// The call failed.
    Call(CallError),
}

/// What switching USB debugging off did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebuggingOff {
    /// The phone answered before the link ended.
    Answered,
    /// The link ended as the setting changed.
    LinkEnded,
    /// No answer in time; whether it changed is for a look at the PC's USB
    /// list to say.
    NoAnswer(CallError),
}

/// What handing the offer through the door did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handed {
    /// The door's command's exit code.
    pub exit: Option<i32>,
    /// What it printed, if anything.
    pub said: String,
}

impl Handed {
    /// True when the command ended well.
    pub fn ok(&self) -> bool {
        self.exit == Some(0) && self.said.trim().is_empty()
    }
}

/// A phone connected over the cable. Dropping it ends the link, and with
/// it any reverse it set up.
pub struct Phone {
    connection: Option<Connection>,
    facts: PhoneFacts,
    waits: Waits,
}

impl std::fmt::Debug for Phone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Phone({} {}, Android {})", self.facts.manufacturer, self.facts.model, self.facts.android)
    }
}

impl Phone {
    /// Connects over `wire` with `key`, and reads what the phone is.
    /// `on_asking` is called when the phone has been shown the key and asks
    /// the person "Allow USB debugging?": from then on the person has
    /// `waits.prompt` to answer.
    pub fn connect(wire: Box<dyn Wire>, key: &AdbKey, waits: Waits, on_asking: &mut dyn FnMut()) -> Result<Phone, OpenProblem> {
        let connection = Connection::connect_waiting(wire, key.private(), &key.public_message(), waits.hello, waits.prompt, on_asking)
            .map_err(|e| match e {
                ConnectError::NotAllowed(_) => OpenProblem::NotAllowed,
                ConnectError::NoAnswer(_) => OpenProblem::NoAnswer,
                ConnectError::WantsTls => OpenProblem::Other("the phone asked for TLS, which only wireless debugging uses".to_string()),
                ConnectError::Lost(why) => OpenProblem::Lost(why),
                ConnectError::Protocol(why) => OpenProblem::Other(why),
            })?;
        let mut phone = Phone { connection: Some(connection), facts: PhoneFacts::default(), waits };
        let text = |phone: &Phone, ask: Ask| phone.shell(ask, None).map(|(text, _)| commands::first_line(&text));
        let lost = |e: CallError| OpenProblem::Lost(e.to_string());
        let facts = PhoneFacts {
            manufacturer: text(&phone, Ask::Manufacturer).map_err(lost)?,
            model: text(&phone, Ask::Model).map_err(lost)?,
            android: text(&phone, Ask::AndroidVersion).map_err(lost)?,
            sdk: text(&phone, Ask::Sdk).map_err(lost)?.parse().ok(),
            usb_debugging: match text(&phone, Ask::UsbDebugging).map_err(lost)?.as_str() {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            },
        };
        phone.facts = facts;
        Ok(phone)
    }

    fn connection(&self) -> Result<&Connection, CallError> {
        self.connection.as_ref().ok_or_else(|| CallError::Lost("the link was ended".to_string()))
    }

    /// Runs one of LiMo's commands; its output (standard output, then
    /// standard error) and its exit code.
    fn shell(&self, ask: Ask, input: Option<&[u8]>) -> Result<(String, Option<i32>), CallError> {
        let output = self.connection()?.shell(&ask.command(), input, self.waits.call).map_err(call_error)?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok((text, output.exit))
    }

    /// What the phone is, as read when it connected.
    pub fn facts(&self) -> &PhoneFacts {
        &self.facts
    }

    /// LiMo's child form on the phone, if it is there.
    pub fn limo(&self) -> Result<Option<LimoOnPhone>, CallError> {
        let (listed, _) = self.shell(Ask::LimoListed, None)?;
        if !commands::lists_limo(&listed) {
            return Ok(None);
        }
        let (dump, _) = self.shell(Ask::LimoVersion, None)?;
        let (version_code, version_name) = commands::parse_version(&dump);
        Ok(Some(LimoOnPhone { version_code, version_name }))
    }

    /// Installs LiMo's child form from the APK at `apk`, replacing it if it
    /// is there. The APK is read on the PC first: anything but LiMo's child
    /// form as a release build is refused with nothing sent. It is read
    /// once, into memory, and the bytes checked are the bytes sent, so a
    /// file changed or cut while it is installed can never put another
    /// build on the phone. Android's refusal comes back as
    /// [`Installed::Refused`], whose code [`commands::install_advice`]
    /// turns into words.
    pub fn install_limo(&self, apk: &Path) -> Result<Installed, InstallError> {
        let bytes = std::fs::read(apk).map_err(|e| InstallError::NotAnApk(ApkProblem::Read(e.to_string())))?;
        let facts = apk_facts(&bytes).map_err(InstallError::NotAnApk)?;
        if facts.package != PACKAGE {
            return Err(InstallError::NotLimo(facts.package));
        }
        if facts.debuggable {
            return Err(InstallError::DebugBuild);
        }
        let answer = self.connection().map_err(InstallError::Call)?.install(&bytes, self.waits.install).map_err(|e| InstallError::Call(call_error(e)))?;
        Ok(commands::parse_install(&answer))
    }

    /// Hands LiMo its pairing offer through its guarded door, from memory:
    /// the bytes go on the standard input of `content write`, never in a
    /// command line, and are wiped here once sent. At most
    /// [`MAX_OFFER_BYTES`].
    pub fn hand_offer(&self, offer: &[u8]) -> Result<Handed, CallError> {
        if offer.is_empty() || offer.len() > MAX_OFFER_BYTES {
            return Err(CallError::Failed(format!("an offer must be 1 to {MAX_OFFER_BYTES} bytes")));
        }
        let offer = Zeroizing::new(offer.to_vec());
        let (said, exit) = self.shell(Ask::OfferIntoDoor, Some(&offer))?;
        Ok(Handed { exit, said })
    }

    /// Carries connections to the link's port on the phone's own loopback
    /// to the same port on this PC's, for as long as this phone is held.
    pub fn reverse_link(&self) -> Result<(), CallError> {
        self.reverse(LINK_PORT)
    }

    /// Removes that again.
    pub fn remove_link_reverse(&self) -> Result<(), CallError> {
        self.remove_reverse(LINK_PORT)
    }

    /// Carries connections to `port` on the phone's own loopback to `port`
    /// on this PC's, for as long as this phone is held. Each connection is
    /// relayed to 127.0.0.1 here, and ends both ways when either side ends
    /// it (adb's streams have no half close).
    pub fn reverse(&self, port: u16) -> Result<(), CallError> {
        self.connection()?.reverse(port, self.waits.call).map_err(call_error)
    }

    /// Removes the reverse of `port`.
    pub fn remove_reverse(&self, port: u16) -> Result<(), CallError> {
        self.connection()?.reverse_remove(port, self.waits.call).map_err(call_error)
    }

    /// Starts LiMo's screen.
    pub fn start_limo(&self) -> Result<Started, CallError> {
        let (text, _) = self.shell(Ask::StartLimo, None)?;
        Ok(commands::parse_start(&text))
    }

    /// Switches USB debugging off on the phone. The link ends there: the
    /// phone is let go, and nothing reaches it over the cable again until a
    /// person switches USB debugging on again on the phone.
    pub fn debugging_off(mut self) -> DebuggingOff {
        let result = self.shell(Ask::DebuggingOff, None);
        if let Some(connection) = self.connection.take() {
            connection.close();
        }
        match result {
            Ok(_) => DebuggingOff::Answered,
            Err(CallError::Lost(_)) => DebuggingOff::LinkEnded,
            Err(other) => DebuggingOff::NoAnswer(other),
        }
    }

    /// Lets go of the phone: the link ends, with any reverse.
    pub fn close(mut self) {
        if let Some(connection) = self.connection.take() {
            connection.close();
        }
    }
}
