//! The channel between the service and a helper: what each may say, and how
//! it travels.
//!
//! A helper is born with its channel: two anonymous pipes the service made,
//! one each way, handed to the helper as inherited handles when it starts.
//! Nothing else can open them. Each message travels as a frame: the version
//! of this protocol (2 bytes), the length of what follows (4 bytes), both
//! little endian, then that many bytes of JSON. A frame longer than
//! [`MAX_MESSAGE_BYTES`] is refused from its header alone, before any of it
//! is kept.
//!
//! A guardian's PIN and a one time code travel over this channel, from the
//! helper to the service. They are carried as a [`Secret`], which never
//! shows in `Debug` output and is wiped from memory when dropped, and the
//! buffers that held their frames are wiped too. Error messages about a
//! frame never quote any part of it.

#![forbid(unsafe_code)]

use std::fmt;

use limo_core::{PinAction, TimeRequest, TryOutcome, WarningCause};
use limo_agent::Show;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// The version of this protocol. A frame of any other version closes the
/// channel. Version 2 added the program's name to a notice, and the
/// helper's word that it shows the plain display.
pub const PROTOCOL_VERSION: u16 = 2;

/// The longest reason a helper gives for showing the plain display that the
/// service keeps, in characters.
pub const MAX_WHY_CHARS: usize = 300;

/// The bytes in front of each message: the version and the length.
pub const HEADER_BYTES: usize = 6;

/// The longest message, in bytes of JSON: 64 KiB. The longest real one, a
/// lock screen with the child's newest request on it, is under 2 KiB.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// The most a [`Decoder`] ever holds: one whole frame and the start of the
/// next.
const MAX_BUFFERED: usize = 2 * (HEADER_BYTES + MAX_MESSAGE_BYTES);

/// One frame, ready to write. Wiped from memory when dropped.
pub type Frame = Zeroizing<Vec<u8>>;

/// A PIN or a one time code as it was typed. Its `Debug` output hides it,
/// and it is wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps what was typed.
    pub fn new(typed: String) -> Secret {
        Secret(typed)
    }

    /// What was typed, to hand to the engine for the one call that needs
    /// it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(hidden)")
    }
}

/// What a helper says to the service.
///
/// The session a message belongs to is always the session the service
/// started that helper in, never anything a message says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum FromHelper {
    /// The first message, and only the first: the protocol it speaks.
    Hello {
        /// Must be [`PROTOCOL_VERSION`].
        protocol: u16,
    },
    /// What the helper got when it started, for the service's log, once:
    /// its UIAccess flag, its integrity level, the z order band of a topmost
    /// window of its own, and its process object's security descriptor.
    About {
        /// The UIAccess flag on its token, if it could be read.
        uiaccess: Option<bool>,
        /// Its integrity level (0x2100 is medium plus), if it could be read.
        integrity: Option<u32>,
        /// The z order band of a topmost window (2 is the UIAccess band), if
        /// it could be read.
        band: Option<u32>,
        /// Its process object's security descriptor, as SDDL, if it could be
        /// read.
        process_sddl: Option<String>,
    },
    /// About once a second: what only a program inside the session can see.
    Report {
        /// Milliseconds since the last keyboard or mouse input.
        idle_ms: u64,
        /// The process id of the window in front, if there is one. The
        /// service finds the program itself, and only in this session.
        front_pid: Option<u32>,
        /// True when something is in full screen.
        full_screen: bool,
    },
    /// The lock handed over with this number is on the screen now.
    LockOnScreen {
        /// The number of the show that put it up.
        seq: u64,
    },
    /// A warning is on the screen now, named by what it is about.
    WarningShown {
        /// What the lock will be for.
        cause: WarningCause,
        /// The warning point, in minutes.
        minutes: u32,
        /// When the lock is expected, unix seconds.
        lock_at: i64,
    },
    /// The child asks for more time today.
    AskForTime {
        /// A number the helper chose; the answer carries it back.
        ask: u32,
        /// The minutes asked for.
        minutes: u32,
        /// The child's reason, possibly empty.
        reason: String,
    },
    /// A one time code typed on the lock screen.
    TryCode {
        /// A number the helper chose; the answer carries it back.
        ask: u32,
        /// The code as typed.
        code: Secret,
    },
    /// A guardian typed the PIN on the lock screen.
    TryPin {
        /// A number the helper chose; the answer carries it back.
        ask: u32,
        /// The PIN as typed.
        pin: Secret,
        /// What the PIN is to do.
        action: PinAction,
    },
    /// The helper shows the plain display, not the drawn one, and why: the
    /// drawn one could not start, or failed, or the settings file asks for
    /// the plain one. For the service's log; it changes nothing else.
    ShowsPlain {
        /// Why, in plain words.
        why: String,
    },
}

/// What the service says to a helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToHelper {
    /// What to show now. Repeated about every five seconds, and sent at once
    /// whenever it changes; `seq` grows with every change.
    Show {
        /// The number of this show.
        seq: u64,
        /// The child's name, when the session is a child's.
        child_name: Option<String>,
        /// What to show.
        show: Show,
    },
    /// A program was closed: show why, for a while.
    Notice {
        /// The reason, in plain words, as one sentence.
        text: String,
        /// The program's name, as the child knows it, when the notice is
        /// about one program: "Game.exe". A drawn notice says "Game.exe was
        /// closed." with the reason under it.
        program: Option<String>,
    },
    /// The answer to a way out.
    Answer {
        /// The number the helper gave the way out.
        ask: u32,
        /// The answer.
        answer: Answer,
    },
    /// Close everything and end.
    Quit,
}

/// The answer to a way out from a lock screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The request for more time was made, and has gone to the guardian.
    Asked {
        /// The request.
        request: TimeRequest,
    },
    /// The code or the PIN was tried.
    Tried {
        /// What became of it.
        outcome: TryOutcome,
    },
    /// It was not tried, for this reason, in plain words.
    Refused {
        /// Why.
        message: String,
    },
}

/// Why a channel was closed. No variant holds any part of what was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelProblem {
    /// A frame of another version of the protocol.
    WrongVersion {
        /// The version it said.
        got: u16,
    },
    /// A frame longer than [`MAX_MESSAGE_BYTES`], or more bytes at once than
    /// a channel may hold.
    TooBig {
        /// The length it said, or the bytes waiting.
        bytes: usize,
    },
    /// A frame that is not a message this side knows. Only where in the
    /// JSON reading stopped is kept, never what was there.
    Malformed {
        /// The line in the JSON.
        line: usize,
        /// The column in that line.
        column: usize,
    },
    /// A message that may not come now, such as a report before hello.
    Unexpected {
        /// What was wrong, in plain words.
        what: String,
    },
    /// Reading from the pipe failed with this Win32 error.
    ReadFailed {
        /// The error number.
        code: u32,
    },
    /// A message could not be written as JSON.
    CannotWrite,
}

impl fmt::Display for ChannelProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelProblem::WrongVersion { got } => write!(
                f,
                "it spoke version {got} of the channel, and this LiMo speaks version {PROTOCOL_VERSION}"
            ),
            ChannelProblem::TooBig { bytes } => write!(
                f,
                "it sent a message of {bytes} bytes, more than the {MAX_MESSAGE_BYTES} a message may have"
            ),
            ChannelProblem::Malformed { line, column } => write!(
                f,
                "it sent a message LiMo cannot read (the JSON stops making sense at line {line}, column {column})"
            ),
            ChannelProblem::Unexpected { what } => write!(f, "{what}"),
            ChannelProblem::ReadFailed { code } => write!(f, "reading from its channel failed with Win32 error {code}"),
            ChannelProblem::CannotWrite => write!(f, "a message to it could not be written"),
        }
    }
}

impl std::error::Error for ChannelProblem {}

/// One message as a frame, ready to write. The buffer is made big enough
/// for the longest message at once, so that it never grows and leaves a copy
/// behind, and it is wiped when dropped.
pub fn encode<T: Serialize>(message: &T) -> Result<Frame, ChannelProblem> {
    let mut frame: Frame = Zeroizing::new(Vec::with_capacity(HEADER_BYTES + MAX_MESSAGE_BYTES + 1));
    frame.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    frame.extend_from_slice(&[0; 4]);
    serde_json::to_writer(&mut *frame, message).map_err(|_| ChannelProblem::CannotWrite)?;
    let length = frame.len() - HEADER_BYTES;
    if length > MAX_MESSAGE_BYTES {
        return Err(ChannelProblem::TooBig { bytes: length });
    }
    let said = u32::try_from(length).map_err(|_| ChannelProblem::TooBig { bytes: length })?;
    frame[2..HEADER_BYTES].copy_from_slice(&said.to_le_bytes());
    Ok(frame)
}

/// Reads frames out of the bytes a pipe gives, in whatever pieces they come.
///
/// Its buffer is made at its full size once, so it never grows and leaves a
/// copy behind; every frame read is wiped from it, and all of it when it is
/// dropped. After it returns a problem, it must not be used again: the
/// channel is closed.
pub struct Decoder {
    buffer: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder").field("waiting_bytes", &self.buffer.len()).finish()
    }
}

impl Default for Decoder {
    fn default() -> Decoder {
        Decoder::new()
    }
}

impl Decoder {
    /// An empty decoder.
    pub fn new() -> Decoder {
        Decoder { buffer: Zeroizing::new(Vec::with_capacity(MAX_BUFFERED)) }
    }

    /// Bytes as they came from the pipe. Refused when more is waiting than
    /// a channel may hold.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ChannelProblem> {
        let waiting = self.buffer.len().saturating_add(bytes.len());
        if waiting > MAX_BUFFERED {
            return Err(ChannelProblem::TooBig { bytes: waiting });
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// The next whole message, if one is waiting. A frame of the wrong
    /// version or too long is refused from its header alone.
    pub fn next_message<T: DeserializeOwned>(&mut self) -> Result<Option<T>, ChannelProblem> {
        if self.buffer.len() < HEADER_BYTES {
            return Ok(None);
        }
        let version = u16::from_le_bytes([self.buffer[0], self.buffer[1]]);
        if version != PROTOCOL_VERSION {
            return Err(ChannelProblem::WrongVersion { got: version });
        }
        let said = u32::from_le_bytes([self.buffer[2], self.buffer[3], self.buffer[4], self.buffer[5]]);
        let length = usize::try_from(said).unwrap_or(usize::MAX);
        if length > MAX_MESSAGE_BYTES {
            return Err(ChannelProblem::TooBig { bytes: length });
        }
        let end = HEADER_BYTES + length;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let parsed = serde_json::from_slice::<T>(&self.buffer[HEADER_BYTES..end]);
        self.consume(end);
        match parsed {
            Ok(message) => Ok(Some(message)),
            Err(e) => Err(ChannelProblem::Malformed { line: e.line(), column: e.column() }),
        }
    }

    /// Bytes waiting that are not yet a whole frame.
    pub fn waiting(&self) -> usize {
        self.buffer.len()
    }

    /// Drops the first `n` bytes, wiping them and the place the rest moved
    /// from.
    fn consume(&mut self, n: usize) {
        let n = n.min(self.buffer.len());
        self.buffer[..n].zeroize();
        let rest = self.buffer.len() - n;
        self.buffer.copy_within(n.., 0);
        self.buffer[rest..].zeroize();
        self.buffer.truncate(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_agent::{LockScreen, LockWhy};

    fn frame_of(json: &str) -> Vec<u8> {
        let mut bytes = PROTOCOL_VERSION.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    fn every_message_from_a_helper() -> Vec<FromHelper> {
        vec![
            FromHelper::Hello { protocol: PROTOCOL_VERSION },
            FromHelper::About {
                uiaccess: Some(true),
                integrity: Some(0x2100),
                band: Some(2),
                process_sddl: Some("O:BAD:P(A;;GA;;;SY)".into()),
            },
            FromHelper::Report { idle_ms: 1_500, front_pid: Some(4_242), full_screen: true },
            FromHelper::LockOnScreen { seq: 7 },
            FromHelper::WarningShown { cause: WarningCause::TimeRunsOut, minutes: 5, lock_at: 1_800_000_000 },
            FromHelper::AskForTime { ask: 1, minutes: 20, reason: "For the school project".into() },
            FromHelper::TryCode { ask: 2, code: Secret::new("1342 0715".into()) },
            FromHelper::TryPin { ask: 3, pin: Secret::new("482915".into()), action: PinAction::AddMinutes { minutes: 20 } },
            FromHelper::ShowsPlain { why: "the settings file asks for the plain display".into() },
        ]
    }

    fn every_message_to_a_helper() -> Vec<ToHelper> {
        let screen = LockScreen {
            why: LockWhy::TimeUsedUp,
            title: LockWhy::TimeUsedUp.title().into(),
            message: LockWhy::TimeUsedUp.message().into(),
            until: Some(1_800_000_000),
            can_ask_for_time: true,
            can_use_code: true,
            can_use_pin: true,
            try_wait_until: None,
            latest_request: None,
        };
        vec![
            ToHelper::Show { seq: 1, child_name: Some("Asha".into()), show: Show::Locked { screen } },
            ToHelper::Show {
                seq: 2,
                child_name: None,
                show: Show::TimeLeft { seconds_left: Some(600), counting: true, warning: None, unlocked_until: None },
            },
            ToHelper::Notice {
                text: "game.exe is blocked by your family's rules, so LiMo closed it.".into(),
                program: Some("game.exe".into()),
            },
            ToHelper::Notice { text: "A program was closed.".into(), program: None },
            ToHelper::Answer { ask: 3, answer: Answer::Tried { outcome: TryOutcome::Malformed } },
            ToHelper::Answer { ask: 4, answer: Answer::Refused { message: "Not now.".into() } },
            ToHelper::Quit,
        ]
    }

    #[test]
    fn every_message_goes_through_a_frame_and_back_unchanged() {
        let mut decoder = Decoder::new();
        for message in every_message_from_a_helper() {
            let frame = encode(&message).unwrap();
            assert_eq!(u16::from_le_bytes([frame[0], frame[1]]), PROTOCOL_VERSION);
            decoder.push(&frame).unwrap();
            assert_eq!(decoder.next_message::<FromHelper>().unwrap(), Some(message));
            assert_eq!(decoder.waiting(), 0);
        }
        for message in every_message_to_a_helper() {
            decoder.push(&encode(&message).unwrap()).unwrap();
            assert_eq!(decoder.next_message::<ToHelper>().unwrap(), Some(message));
        }
        assert_eq!(decoder.next_message::<ToHelper>().unwrap(), None);
    }

    #[test]
    fn frames_come_whole_in_any_pieces_and_several_at_once() {
        let mut all = Vec::new();
        for message in every_message_from_a_helper() {
            all.extend_from_slice(&encode(&message).unwrap());
        }
        // One byte at a time.
        let mut decoder = Decoder::new();
        let mut got = Vec::new();
        for byte in &all {
            decoder.push(std::slice::from_ref(byte)).unwrap();
            while let Some(message) = decoder.next_message::<FromHelper>().unwrap() {
                got.push(message);
            }
        }
        assert_eq!(got, every_message_from_a_helper());
        // All at once.
        let mut decoder = Decoder::new();
        decoder.push(&all).unwrap();
        let mut got = Vec::new();
        while let Some(message) = decoder.next_message::<FromHelper>().unwrap() {
            got.push(message);
        }
        assert_eq!(got, every_message_from_a_helper());
    }

    #[test]
    fn a_frame_too_long_is_refused_from_its_header_alone() {
        let mut decoder = Decoder::new();
        let mut header = PROTOCOL_VERSION.to_le_bytes().to_vec();
        header.extend_from_slice(&((MAX_MESSAGE_BYTES as u32) + 1).to_le_bytes());
        decoder.push(&header).unwrap();
        assert_eq!(decoder.next_message::<FromHelper>(), Err(ChannelProblem::TooBig { bytes: MAX_MESSAGE_BYTES + 1 }));
        let mut decoder = Decoder::new();
        let mut header = PROTOCOL_VERSION.to_le_bytes().to_vec();
        header.extend_from_slice(&u32::MAX.to_le_bytes());
        decoder.push(&header).unwrap();
        assert!(matches!(decoder.next_message::<FromHelper>(), Err(ChannelProblem::TooBig { .. })));
        // A flood of bytes is refused before it is kept.
        let mut decoder = Decoder::new();
        assert!(matches!(decoder.push(&vec![0u8; MAX_BUFFERED + 1]), Err(ChannelProblem::TooBig { .. })));
        assert_eq!(decoder.waiting(), 0);
        // The longest message allowed goes through.
        let reason = "x".repeat(MAX_MESSAGE_BYTES - 200);
        let longest = FromHelper::AskForTime { ask: 1, minutes: 5, reason };
        let frame = encode(&longest).unwrap();
        let mut decoder = Decoder::new();
        decoder.push(&frame).unwrap();
        assert_eq!(decoder.next_message::<FromHelper>().unwrap(), Some(longest));
        // One longer cannot even be written.
        let too_long = FromHelper::AskForTime { ask: 1, minutes: 5, reason: "x".repeat(MAX_MESSAGE_BYTES) };
        assert!(matches!(encode(&too_long), Err(ChannelProblem::TooBig { .. })));
    }

    #[test]
    fn another_version_garbage_and_unknown_messages_are_refused() {
        let mut decoder = Decoder::new();
        let mut frame = frame_of(r#"{"say":"hello","protocol":1}"#);
        frame[0] = 9;
        decoder.push(&frame).unwrap();
        assert_eq!(decoder.next_message::<FromHelper>(), Err(ChannelProblem::WrongVersion { got: 9 }));

        for json in [
            "not json at all",
            r#"{"say":"take_over_the_pc"}"#,
            r#"{"say":"hello"}"#,
            r#"{"say":"hello","protocol":1,"extra":true}"#,
            r#"{"say":"report","idle_ms":-5,"front_pid":null,"full_screen":false}"#,
            r#"{"say":"report","idle_ms":1,"front_pid":null,"full_screen":false,"window_title":"x"}"#,
            "",
        ] {
            let mut decoder = Decoder::new();
            decoder.push(&frame_of(json)).unwrap();
            assert!(matches!(decoder.next_message::<FromHelper>(), Err(ChannelProblem::Malformed { .. })), "{json}");
        }
    }

    #[test]
    fn a_pin_never_shows_in_debug_output_or_in_an_error() {
        let message = FromHelper::TryPin { ask: 1, pin: Secret::new("482915".into()), action: PinAction::ConfirmRelease };
        let shown = format!("{message:?}");
        assert!(!shown.contains("482915"), "{shown}");
        assert!(shown.contains("Secret(hidden)"), "{shown}");
        let code = FromHelper::TryCode { ask: 1, code: Secret::new("13420715".into()) };
        assert!(!format!("{code:?}").contains("1342"));
        // A PIN sent as a number where text belongs: the error says where,
        // never what.
        let mut decoder = Decoder::new();
        decoder.push(&frame_of(r#"{"say":"try_pin","ask":1,"pin":482915,"action":"confirm_release"}"#)).unwrap();
        let problem = decoder.next_message::<FromHelper>().unwrap_err();
        for text in [problem.to_string(), format!("{problem:?}")] {
            assert!(!text.contains("482915") && !text.contains("4829"), "{text}");
        }
        // And the decoder says nothing of what it holds.
        let mut decoder = Decoder::new();
        decoder.push(&frame_of(r#"{"say":"try_pin","ask":1,"pin":"482915""#)).unwrap();
        assert!(!format!("{decoder:?}").contains("4829"));
    }

    #[test]
    fn what_was_read_is_wiped_from_the_decoder() {
        let mut decoder = Decoder::new();
        let pin = FromHelper::TryPin { ask: 1, pin: Secret::new("482915".into()), action: PinAction::ConfirmRelease };
        let mut both = encode(&pin).unwrap().to_vec();
        both.extend_from_slice(&encode(&FromHelper::Hello { protocol: 1 }).unwrap());
        decoder.push(&both[..both.len() - 3]).unwrap();
        assert!(decoder.next_message::<FromHelper>().unwrap().is_some());
        // Only the start of the hello is left, and no byte of the PIN frame.
        assert_eq!(decoder.waiting(), both.len() - 3 - encode(&pin).unwrap().len());
        assert!(!decoder.buffer.windows(6).any(|w| w == b"482915"));
        both.zeroize();
    }
}
