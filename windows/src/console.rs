//! The console's door: what the guardian's console may say to the service,
//! what it is answered, and how it travels.
//!
//! The console (a window in a guardian's own session) never opens the
//! family's file: the service holds it alone, on its one thread. The console
//! asks the service through a named pipe, [`CONSOLE_PIPE_NAME`], which only
//! the service makes. Each message travels as a frame, as on the helper's
//! channel: the protocol version (2 bytes), the length of what follows (4
//! bytes), both little endian, then that many bytes of JSON, at most
//! [`MAX_CONSOLE_MESSAGE_BYTES`].
//!
//! The console says hello first, then sends requests, each with a number of
//! its own that the answer carries back. Every request maps onto one call of
//! the family's file ([`Request`]). A password or a PIN travels as a
//! [`Secret`], which never shows in `Debug` output and is wiped from memory
//! when dropped; the frames that carried it are wiped too.

#![forbid(unsafe_code)]

use limo_core::{RequestReply, RuleProblem, RuleSet};
use limo_hub::{AccountLink, AgeBand, UnlinkedAccounts};
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use crate::protocol::{ChannelProblem, Decoder, Frame, Secret, encode_frame};

/// The door's name. Only the service makes it, and it refuses clients from
/// other PCs.
pub const CONSOLE_PIPE_NAME: &str = r"\\.\pipe\limo.console";

/// The version of the console's protocol. A frame of any other version
/// closes the connection.
pub const CONSOLE_PROTOCOL_VERSION: u16 = 1;

/// The longest message either way, in bytes of JSON: 1 MiB. A child's usage
/// over many days, with every app in it, is the longest real answer.
pub const MAX_CONSOLE_MESSAGE_BYTES: usize = 1024 * 1024;

/// A console must send its first bytes, and then its hello, each within
/// this long, in milliseconds, or it is closed.
pub const HELLO_WAIT_MS: u64 = 10_000;

/// The most messages a console may send in one second. One more closes the
/// connection: a screen asks again every few seconds, never this often.
pub const MAX_MESSAGES_PER_SECOND: u32 = 20;

/// What the console says to the service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum FromConsole {
    /// The first message, and only the first: the protocol it speaks.
    Hello {
        /// Must be [`CONSOLE_PROTOCOL_VERSION`].
        protocol: u16,
    },
    /// A request, with a number the console chose; the answer carries it
    /// back.
    Request {
        /// The console's number for it.
        id: u32,
        /// What is asked.
        request: Request,
    },
}

/// One request: its name and its arguments, as `{"name": ..., "args":
/// {...}}`. Each maps onto one call of the family's file. Children, devices,
/// requests and accounts are named by the ids the service gave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "args", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// Signs in with the guardian password.
    SignIn {
        /// The password as typed.
        password: Secret,
    },
    /// Ends this connection's session.
    SignOut,
    /// LiMo for Windows' version, the protocol's, and the PC's name.
    About,
    /// Whether a family is set up, whether it has a PIN, how many children,
    /// and what happens in an account not set up.
    FamilyState,
    /// Makes the family with its guardian password, when there is none yet;
    /// it signs this connection in.
    CreateFamily {
        /// The new password.
        password: Secret,
    },
    /// Sets or changes the guardian PIN.
    SetPin {
        /// The new PIN, 6 to 8 digits.
        pin: Secret,
    },
    /// Changes the guardian password.
    ChangePassword {
        /// The password now.
        old: Secret,
        /// The new password.
        new: Secret,
    },
    /// Every child's day.
    FamilyToday,
    /// One child's day.
    ChildToday {
        /// The child.
        child: String,
    },
    /// Adds a child, in the PC's own time zone.
    AddChild {
        /// The child's name, 1 to 40 characters.
        name: String,
        /// The child's age band.
        age_band: AgeBand,
    },
    /// Renames a child.
    RenameChild {
        /// The child.
        child: String,
        /// The new name.
        name: String,
    },
    /// Changes a child's age band.
    SetAgeBand {
        /// The child.
        child: String,
        /// The new band.
        age_band: AgeBand,
    },
    /// Removes a child, with everything kept for them: the password again.
    RemoveChild {
        /// The child.
        child: String,
        /// The guardian password, again.
        password: Secret,
    },
    /// A child's rules.
    Rules {
        /// The child.
        child: String,
    },
    /// Replaces a child's rules; refused with the rules' own problems.
    ReplaceRules {
        /// The child.
        child: String,
        /// The new rules.
        rules: RuleSet,
    },
    /// A child's usage over home days.
    Usage {
        /// The child.
        child: String,
        /// The first home day.
        from_day: i64,
        /// The last home day.
        to_day: i64,
    },
    /// Every request for more time that waits for an answer.
    WaitingRequests,
    /// Answers a child's request for more time.
    AnswerRequest {
        /// The child.
        child: String,
        /// The request's id.
        request: String,
        /// The answer.
        reply: RequestReply,
    },
    /// Gives a child extra time today, once for each tap.
    GiveExtra {
        /// The child.
        child: String,
        /// The minutes.
        minutes: u32,
        /// The screen's own id for the tap.
        tap_id: String,
    },
    /// Locks all of a child's devices, until a time or until lifted.
    LockNow {
        /// The child.
        child: String,
        /// Until when, unix seconds, or none for until lifted.
        #[serde(default)]
        until: Option<i64>,
    },
    /// Opens all of a child's devices until a time.
    UnlockUntil {
        /// The child.
        child: String,
        /// Until when, unix seconds.
        until: i64,
    },
    /// Lifts a lock or an unlock.
    Lift {
        /// The child.
        child: String,
    },
    /// Makes a one time code for a device.
    MakeCode {
        /// The device.
        device: String,
        /// 15, 30 or 60.
        minutes: u32,
    },
    /// A child's devices.
    Devices {
        /// The child.
        child: String,
    },
    /// One device.
    Device {
        /// The device.
        device: String,
    },
    /// Renames a device.
    RenameDevice {
        /// The device.
        device: String,
        /// The new name.
        name: String,
    },
    /// Removes a device: the password again.
    RemoveDevice {
        /// The device.
        device: String,
        /// The guardian password, again.
        password: Secret,
    },
    /// Every Windows account on this PC, with its link.
    Accounts,
    /// Links a Windows account of this PC to a child, or marks it as not
    /// limited.
    LinkAccount {
        /// The account's security identifier.
        account: String,
        /// The link.
        link: AccountLink,
    },
    /// Takes a Windows account's link away.
    UnlinkAccount {
        /// The account's security identifier.
        account: String,
    },
    /// The PC's settings.
    PcSettings,
    /// After how many idle minutes the count pauses.
    SetIdleMinutes {
        /// 1 to 60.
        minutes: u32,
    },
    /// What happens in an account not set up.
    SetUnlinkedAccounts {
        /// Free or locked.
        unlinked: UnlinkedAccounts,
    },
    /// The end of the first start: accounts not set up are locked from now
    /// on (administrators never are).
    FinishFirstStart,
    /// The newest events.
    Events {
        /// How many, at most.
        limit: u32,
        /// Only those older than this event id.
        #[serde(default)]
        before: Option<i64>,
    },
    /// Allows LiMo to be removed from this PC for a while: the password
    /// again.
    AllowRemoval {
        /// The guardian password, again.
        password: Secret,
    },
    /// Deletes the whole family: the password again.
    DeleteFamily {
        /// The guardian password, again.
        password: Secret,
    },
    /// A pairing offer for a child's phone. Not built yet.
    MakeOffer {
        /// The child.
        child: String,
    },
    /// A device's open link sessions and the way each came. Not built yet.
    DeviceLink {
        /// The device.
        device: String,
    },
}

impl Request {
    /// The request's name, as the console sends it: for the log.
    pub fn name(&self) -> &'static str {
        match self {
            Request::SignIn { .. } => "sign_in",
            Request::SignOut => "sign_out",
            Request::About => "about",
            Request::FamilyState => "family_state",
            Request::CreateFamily { .. } => "create_family",
            Request::SetPin { .. } => "set_pin",
            Request::ChangePassword { .. } => "change_password",
            Request::FamilyToday => "family_today",
            Request::ChildToday { .. } => "child_today",
            Request::AddChild { .. } => "add_child",
            Request::RenameChild { .. } => "rename_child",
            Request::SetAgeBand { .. } => "set_age_band",
            Request::RemoveChild { .. } => "remove_child",
            Request::Rules { .. } => "rules",
            Request::ReplaceRules { .. } => "replace_rules",
            Request::Usage { .. } => "usage",
            Request::WaitingRequests => "waiting_requests",
            Request::AnswerRequest { .. } => "answer_request",
            Request::GiveExtra { .. } => "give_extra",
            Request::LockNow { .. } => "lock_now",
            Request::UnlockUntil { .. } => "unlock_until",
            Request::Lift { .. } => "lift",
            Request::MakeCode { .. } => "make_code",
            Request::Devices { .. } => "devices",
            Request::Device { .. } => "device",
            Request::RenameDevice { .. } => "rename_device",
            Request::RemoveDevice { .. } => "remove_device",
            Request::Accounts => "accounts",
            Request::LinkAccount { .. } => "link_account",
            Request::UnlinkAccount { .. } => "unlink_account",
            Request::PcSettings => "pc_settings",
            Request::SetIdleMinutes { .. } => "set_idle_minutes",
            Request::SetUnlinkedAccounts { .. } => "set_unlinked_accounts",
            Request::FinishFirstStart => "finish_first_start",
            Request::Events { .. } => "events",
            Request::AllowRemoval { .. } => "allow_removal",
            Request::DeleteFamily { .. } => "delete_family",
            Request::MakeOffer { .. } => "make_offer",
            Request::DeviceLink { .. } => "device_link",
        }
    }

    /// True when it may be asked without signing in: what the first screen
    /// needs to know, and making the family while there is none.
    pub fn open_to_anyone(&self) -> bool {
        matches!(
            self,
            Request::SignIn { .. } | Request::SignOut | Request::About | Request::FamilyState | Request::CreateFamily { .. }
        )
    }

    /// True when it carries the guardian password or the PIN, which is
    /// checked or hashed slowly on purpose: at most one a second is taken
    /// from a connection, and at most one between two steps of the service.
    pub fn carries_a_secret(&self) -> bool {
        matches!(
            self,
            Request::SignIn { .. }
                | Request::CreateFamily { .. }
                | Request::SetPin { .. }
                | Request::ChangePassword { .. }
                | Request::RemoveChild { .. }
                | Request::RemoveDevice { .. }
                | Request::AllowRemoval { .. }
                | Request::DeleteFamily { .. }
        )
    }

    /// True for the looks a screen repeats by itself every few seconds while
    /// it is open. They do not keep a session open: a session ends fifteen
    /// minutes after the last request that is not one of them.
    pub fn is_repeated_look(&self) -> bool {
        matches!(
            self,
            Request::FamilyToday
                | Request::ChildToday { .. }
                | Request::WaitingRequests
                | Request::Devices { .. }
                | Request::Device { .. }
                | Request::DeviceLink { .. }
                | Request::About
                | Request::FamilyState
        )
    }
}

/// What the service says to a console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToConsole {
    /// The answer to hello: the protocol, and LiMo for Windows' version.
    Hello {
        /// [`CONSOLE_PROTOCOL_VERSION`].
        protocol: u16,
        /// LiMo for Windows' version, such as "0.1.0".
        version: String,
    },
    /// The door does not open for this connection, and why, in a sentence
    /// for the guardian's screen. The connection is closed after it.
    Refused {
        /// Why.
        message: String,
    },
    /// The answer to a request.
    Answer {
        /// The number the console gave the request.
        id: u32,
        /// What became of it.
        reply: Reply,
    },
}

/// What became of a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    /// Done: what the family's file gave back, as JSON, in the hub's own
    /// shapes (for a request that gives nothing back, an empty object).
    Done {
        /// The value.
        value: serde_json::Value,
    },
    /// Not done, and why.
    Problem {
        /// Why.
        problem: Problem,
    },
}

/// Why a request was not done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// The connection is not signed in, or its session has ended: sign in.
    SignedOut,
    /// The guardian password was wrong. Counted.
    WrongPassword,
    /// Wrong passwords have set a wait; nothing was checked.
    MustWait,
    /// Something given cannot be taken, such as a name too long.
    Invalid,
    /// The rules were not taken; their problems say why, field by field.
    Rules,
    /// No such child, device, request or account.
    NotFound,
    /// Not allowed now, such as a family made twice.
    Refused,
    /// One thing at a time: try again in a moment.
    Busy,
    /// Not built into LiMo for Windows yet.
    NotYet,
    /// A request this LiMo does not know, or with arguments it cannot read.
    UnknownRequest,
    /// No family is set up yet.
    NoFamily,
    /// The answer would be longer than a message may be: ask for less.
    TooBig,
    /// The family's file or Windows failed; nothing was changed.
    Failed,
}

/// Why a request was not done, for the guardian's screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    /// What kind of problem.
    pub kind: ProblemKind,
    /// A sentence for the guardian.
    pub message: String,
    /// When a wait after wrong guesses ends, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<i64>,
    /// How many more wrong tries before a wait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tries_before_wait: Option<u32>,
    /// The rules' own problems, each with its field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_problems: Vec<RuleProblem>,
}

impl Problem {
    /// A problem with its sentence only.
    pub fn new(kind: ProblemKind, message: impl Into<String>) -> Problem {
        Problem { kind, message: message.into(), wait_until: None, tries_before_wait: None, rule_problems: Vec::new() }
    }
}

/// What one frame from a console was.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleIncoming {
    /// A message.
    Message(FromConsole),
    /// A request of the right shape around one this LiMo cannot read: an
    /// unknown name, or arguments it cannot take. It is answered, by its
    /// number, and the connection stays open.
    BadRequest {
        /// The console's number for it.
        id: u32,
    },
}

/// A request's envelope with what it asks left unread, to answer a request
/// that cannot be read by its number. Nothing of what it asks is kept.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    #[serde(rename = "say")]
    _say: RequestWord,
    id: u32,
    #[serde(rename = "request")]
    _request: IgnoredAny,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RequestWord {
    Request,
}

/// Reads one frame's JSON from a console. A frame that is not a message
/// at all closes the connection; only where the JSON stopped making sense is
/// kept, never what was there.
pub fn read_console_frame(json: &[u8]) -> Result<ConsoleIncoming, ChannelProblem> {
    match serde_json::from_slice::<FromConsole>(json) {
        Ok(message) => Ok(ConsoleIncoming::Message(message)),
        Err(first) => match serde_json::from_slice::<Envelope>(json) {
            Ok(envelope) => Ok(ConsoleIncoming::BadRequest { id: envelope.id }),
            Err(_) => Err(ChannelProblem::Malformed { line: first.line(), column: first.column() }),
        },
    }
}

/// Reads frames from a console out of the bytes a pipe gives.
#[derive(Debug, Default)]
pub struct ConsoleReader {
    decoder: Option<Decoder>,
}

impl ConsoleReader {
    /// An empty reader.
    pub fn new() -> ConsoleReader {
        ConsoleReader { decoder: Some(Decoder::for_frames(CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES)) }
    }

    fn decoder(&mut self) -> &mut Decoder {
        self.decoder.get_or_insert_with(|| Decoder::for_frames(CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES))
    }

    /// Bytes as they came from the pipe.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ChannelProblem> {
        self.decoder().push(bytes)
    }

    /// The next whole frame, if one is waiting.
    pub fn next_frame(&mut self) -> Result<Option<ConsoleIncoming>, ChannelProblem> {
        self.decoder().next_with(read_console_frame)
    }
}

/// A message to a console, as a frame. A message longer than a frame may be
/// is refused here.
pub fn encode_to_console(message: &ToConsole) -> Result<Frame, ChannelProblem> {
    encode_frame(message, CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES)
}

/// A message from a console, as a frame: for a console, and for tests.
pub fn encode_from_console(message: &FromConsole) -> Result<Frame, ChannelProblem> {
    encode_frame(message, CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES)
}

/// A decoder of the service's answers, for a console and for tests.
pub fn answer_decoder() -> Decoder {
    Decoder::for_frames(CONSOLE_PROTOCOL_VERSION, MAX_CONSOLE_MESSAGE_BYTES)
}

/// Why a connection was closed, in plain words for the log. Never any part
/// of what was sent.
pub fn closed_because(problem: &ChannelProblem) -> String {
    match problem {
        ChannelProblem::WrongVersion { got } => format!(
            "it spoke version {got} of the console's protocol, and this LiMo speaks version {CONSOLE_PROTOCOL_VERSION}"
        ),
        ChannelProblem::TooBig { bytes } => {
            format!("it sent a message of {bytes} bytes, more than the {MAX_CONSOLE_MESSAGE_BYTES} a message may have")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::HEADER_BYTES;

    fn frame_of(json: &str) -> Vec<u8> {
        let mut bytes = CONSOLE_PROTOCOL_VERSION.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    fn read(json: &str) -> Result<Option<ConsoleIncoming>, ChannelProblem> {
        let mut reader = ConsoleReader::new();
        reader.push(&frame_of(json)).unwrap();
        reader.next_frame()
    }

    #[test]
    fn requests_read_by_their_names_and_arguments() {
        let sign_in = r#"{"say":"request","id":3,"request":{"name":"sign_in","args":{"password":"correct horse battery"}}}"#;
        match read(sign_in).unwrap() {
            Some(ConsoleIncoming::Message(FromConsole::Request { id: 3, request: Request::SignIn { password } })) => {
                assert_eq!(password.expose(), "correct horse battery");
            }
            other => panic!("{other:?}"),
        }
        let today = r#"{"say":"request","id":4,"request":{"name":"family_today"}}"#;
        assert!(matches!(
            read(today).unwrap(),
            Some(ConsoleIncoming::Message(FromConsole::Request { id: 4, request: Request::FamilyToday }))
        ));
        let lock = r#"{"say":"request","id":5,"request":{"name":"lock_now","args":{"child":"c1"}}}"#;
        assert!(matches!(
            read(lock).unwrap(),
            Some(ConsoleIncoming::Message(FromConsole::Request { request: Request::LockNow { until: None, .. }, .. }))
        ));
        let link = r#"{"say":"request","id":6,"request":{"name":"link_account","args":{"account":"S-1-5-21-1","link":{"child":{"child_id":"c1"}}}}}"#;
        assert!(matches!(
            read(link).unwrap(),
            Some(ConsoleIncoming::Message(FromConsole::Request { request: Request::LinkAccount { link: AccountLink::Child { .. }, .. }, .. }))
        ));
        let hello = r#"{"say":"hello","protocol":1}"#;
        assert_eq!(read(hello).unwrap(), Some(ConsoleIncoming::Message(FromConsole::Hello { protocol: 1 })));
    }

    #[test]
    fn every_request_goes_through_a_frame_and_back_unchanged() {
        let requests = vec![
            Request::SignIn { password: Secret::new("correct horse battery".into()) },
            Request::SignOut,
            Request::FamilyToday,
            Request::AddChild { name: "Asha".into(), age_band: AgeBand::SixToNine },
            Request::ReplaceRules { child: "c1".into(), rules: RuleSet::new(1, 19_800) },
            Request::AnswerRequest { child: "c1".into(), request: "d/ask/1".into(), reply: RequestReply::Grant { minutes: 30 } },
            Request::SetUnlinkedAccounts { unlinked: UnlinkedAccounts::Locked },
            Request::Events { limit: 20, before: Some(9) },
            Request::MakeOffer { child: "c1".into() },
        ];
        for (n, request) in requests.into_iter().enumerate() {
            let message = FromConsole::Request { id: n as u32, request };
            let frame = encode_from_console(&message).unwrap();
            let mut reader = ConsoleReader::new();
            reader.push(&frame).unwrap();
            assert_eq!(reader.next_frame().unwrap(), Some(ConsoleIncoming::Message(message)));
        }
        let answer = ToConsole::Answer {
            id: 7,
            reply: Reply::Problem { problem: Problem::new(ProblemKind::WrongPassword, "That password is not right.") },
        };
        let mut decoder = answer_decoder();
        decoder.push(&encode_to_console(&answer).unwrap()).unwrap();
        assert_eq!(decoder.next_message::<ToConsole>().unwrap(), Some(answer));
    }

    #[test]
    fn a_request_that_cannot_be_read_is_answered_by_its_number_and_nothing_of_it_kept() {
        for json in [
            r#"{"say":"request","id":8,"request":{"name":"take_over_the_pc"}}"#,
            r#"{"say":"request","id":8,"request":{"name":"lock_now","args":{"child":"c1","until":"soon"}}}"#,
            r#"{"say":"request","id":8,"request":{"name":"lock_now","args":{"child":"c1","window_title":"x"}}}"#,
            r#"{"say":"request","id":8,"request":{"name":"sign_in","args":{"password":482915}}}"#,
            r#"{"say":"request","id":8,"request":{"name":"sign_in"}}"#,
            r#"{"say":"request","id":8,"request":"sign_in"}"#,
        ] {
            assert_eq!(read(json).unwrap(), Some(ConsoleIncoming::BadRequest { id: 8 }), "{json}");
        }
    }

    #[test]
    fn a_frame_that_is_not_a_message_closes_and_says_only_where() {
        for json in [
            "not json at all",
            "",
            r#"{"say":"take_over_the_pc"}"#,
            r#"{"say":"hello"}"#,
            r#"{"say":"hello","protocol":1,"extra":true}"#,
            r#"{"say":"request","request":{"name":"about"}}"#,
            r#"{"say":"request","id":-1,"request":{"name":"about"}}"#,
            r#"{"say":"request","id":1,"request":{"name":"about"},"window_title":"secret diary"}"#,
        ] {
            let problem = read(json).unwrap_err();
            assert!(matches!(problem, ChannelProblem::Malformed { .. }), "{json}: {problem:?}");
            assert!(!closed_because(&problem).contains("secret"), "{json}");
        }
        // Another version, and a frame too long, from the header alone.
        let mut other = frame_of(r#"{"say":"hello","protocol":1}"#);
        other[..2].copy_from_slice(&3u16.to_le_bytes());
        let mut reader = ConsoleReader::new();
        reader.push(&other).unwrap();
        assert_eq!(reader.next_frame(), Err(ChannelProblem::WrongVersion { got: 3 }));
        let mut header = CONSOLE_PROTOCOL_VERSION.to_le_bytes().to_vec();
        header.extend_from_slice(&((MAX_CONSOLE_MESSAGE_BYTES as u32) + 1).to_le_bytes());
        let mut reader = ConsoleReader::new();
        reader.push(&header).unwrap();
        assert_eq!(reader.next_frame(), Err(ChannelProblem::TooBig { bytes: MAX_CONSOLE_MESSAGE_BYTES + 1 }));
        assert!(closed_because(&ChannelProblem::WrongVersion { got: 3 }).contains("console's protocol"));
        // A flood of bytes is refused before it is kept.
        let mut reader = ConsoleReader::new();
        assert!(reader.push(&vec![0u8; 2 * (HEADER_BYTES + MAX_CONSOLE_MESSAGE_BYTES) + 1]).is_err());
    }

    #[test]
    fn a_password_never_shows_in_debug_output_or_in_an_error() {
        let message = FromConsole::Request {
            id: 1,
            request: Request::ChangePassword { old: Secret::new("correct horse".into()), new: Secret::new("battery staple".into()) },
        };
        let shown = format!("{message:?}");
        assert!(!shown.contains("horse") && !shown.contains("staple"), "{shown}");
        let problem = read(r#"{"say":"request","id":1,"request":{"name":"sign_in","args":{"password":"hunter22"}},"x":1}"#).unwrap_err();
        assert!(!format!("{problem:?} {problem}").contains("hunter"));
    }

    #[test]
    fn the_longest_answer_goes_and_a_longer_one_is_refused() {
        let value = serde_json::Value::String("x".repeat(MAX_CONSOLE_MESSAGE_BYTES - 200));
        let answer = ToConsole::Answer { id: 1, reply: Reply::Done { value } };
        assert!(encode_to_console(&answer).is_ok());
        let value = serde_json::Value::String("x".repeat(MAX_CONSOLE_MESSAGE_BYTES));
        let answer = ToConsole::Answer { id: 1, reply: Reply::Done { value } };
        assert!(matches!(encode_to_console(&answer), Err(ChannelProblem::TooBig { .. })));
    }

    #[test]
    fn what_each_request_needs_is_said_by_the_request() {
        assert!(Request::SignIn { password: Secret::new(String::new()) }.open_to_anyone());
        assert!(Request::FamilyState.open_to_anyone() && Request::About.open_to_anyone());
        assert!(!Request::FamilyToday.open_to_anyone() && !Request::FinishFirstStart.open_to_anyone());
        assert!(Request::DeleteFamily { password: Secret::new(String::new()) }.carries_a_secret());
        assert!(!Request::Accounts.carries_a_secret());
        assert!(Request::FamilyToday.is_repeated_look() && !Request::Rules { child: "c".into() }.is_repeated_look());
        assert_eq!(Request::FinishFirstStart.name(), "finish_first_start");
    }
}
