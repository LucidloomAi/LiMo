//! The console's door: what the console and LiMo's service say to each
//! other, and how it travels.
//!
//! The door is the named pipe [`DOOR_PIPE`], which only the service makes.
//! Each message travels as a frame, as on the helper channel of LiMo for
//! Windows: the version of this protocol (2 bytes), the length of what
//! follows (4 bytes), both little endian, then that many bytes of JSON. A
//! frame longer than [`MAX_MESSAGE_BYTES`] is refused from its header alone,
//! before any of it is kept.
//!
//! A guardian's password travels as a [`Secret`], which never shows in
//! `Debug` output and is wiped from memory when dropped; the buffers that
//! held its frame are wiped too. No error about a frame quotes any part of
//! it.
//!
//! Only two requests exist yet: `about` and `sign_in`. A request the console
//! does not know is refused before anything is sent.

#![forbid(unsafe_code)]

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// The door's name. Only LiMo's service makes it; the console checks who
/// did before it says anything (see [`crate::door`]).
pub const DOOR_PIPE: &str = r"\\.\pipe\limo.console";

/// The version of this protocol. A frame of any other version closes the
/// connection.
pub const PROTOCOL_VERSION: u16 = 1;

/// The bytes in front of each message: the version and the length.
pub const HEADER_BYTES: usize = 6;

/// The longest message, in bytes of JSON: 1 MiB, because a phone's list of
/// apps with their names is long.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// The most a [`Decoder`] ever holds: one whole frame and the start of the
/// next.
const MAX_BUFFERED: usize = 2 * (HEADER_BYTES + MAX_MESSAGE_BYTES);

/// One frame, ready to write. Wiped from memory when dropped.
pub type Frame = Zeroizing<Vec<u8>>;

/// A password as it was typed. Its `Debug` output hides it, and it is wiped
/// from memory when dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps what was typed.
    pub fn new(typed: String) -> Secret {
        Secret(typed)
    }

    /// What was typed, for the one comparison that needs it.
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

/// What the console asks the door.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// What answers, and whether a family exists yet. Asked first, so a
    /// child's account is told at once that the settings are not for it.
    About,
    /// A guardian signs in with the family's password.
    SignIn {
        /// As typed.
        password: Secret,
    },
}

impl Request {
    /// The request a page asked for by its name and its arguments, or why
    /// there is none. Nothing about the arguments' values is quoted.
    pub fn from_page(name: &str, args: &serde_json::Value) -> Result<Request, String> {
        let mut object = match args {
            serde_json::Value::Object(map) => map.clone(),
            serde_json::Value::Null => serde_json::Map::new(),
            _ => return Err(format!("the arguments of {name} are not an object")),
        };
        if object.contains_key("request") {
            return Err(format!("the arguments of {name} hold a field named request"));
        }
        object.insert("request".to_string(), serde_json::Value::String(name.to_string()));
        let value = serde_json::Value::Object(object);
        match serde_json::from_value::<Request>(value) {
            Ok(request) => Ok(request),
            Err(_) if !KNOWN.contains(&name) => Err(format!("the console has no request named {name}")),
            Err(_) => Err(format!("the arguments of {name} are not the ones it takes")),
        }
    }

    /// The request's name, for the log. Never its arguments.
    pub fn name(&self) -> &'static str {
        match self {
            Request::About => "about",
            Request::SignIn { .. } => "sign_in",
        }
    }
}

/// Every request's name.
const KNOWN: [&str; 2] = ["about", "sign_in"];

/// What the door answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// What answered.
    About {
        /// The program behind the door, such as "LiMo for Windows".
        service: String,
        /// Its version.
        version: String,
        /// Whether a family has been made on this PC.
        family: bool,
    },
    /// The password was right: this connection is signed in.
    SignedIn,
    /// The password was not right.
    WrongPassword {
        /// How many more wrong tries before a wait.
        tries_before_wait: u32,
    },
    /// Too many wrong tries: no password is looked at until then.
    MustWait {
        /// Until when, in seconds since 1970 (UTC).
        until: i64,
    },
    /// The door will not do this for this connection.
    Refused {
        /// Why.
        why: Refusal,
    },
}

/// Why the door refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "snake_case", deny_unknown_fields)]
pub enum Refusal {
    /// The Windows account the console runs in is a child's: LiMo's
    /// settings are never opened from it.
    ChildAccount {
        /// The child's name, as the family calls the child.
        child: String,
    },
    /// The door could not tell which account the console runs in.
    UnknownAccount,
}

/// Why a connection was closed. No variant holds any part of what was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameProblem {
    /// A frame of another version of the protocol.
    WrongVersion {
        /// The version it said.
        got: u16,
    },
    /// A frame longer than [`MAX_MESSAGE_BYTES`], or more bytes at once than
    /// a connection may hold.
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
    /// A message could not be written as JSON.
    CannotWrite,
}

impl fmt::Display for FrameProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameProblem::WrongVersion { got } => {
                write!(f, "it spoke version {got} of the door, and this console speaks version {PROTOCOL_VERSION}")
            }
            FrameProblem::TooBig { bytes } => {
                write!(f, "it sent a message of {bytes} bytes, more than the {MAX_MESSAGE_BYTES} a message may have")
            }
            FrameProblem::Malformed { line, column } => write!(
                f,
                "it sent a message the console cannot read (the JSON stops making sense at line {line}, column {column})"
            ),
            FrameProblem::CannotWrite => write!(f, "a message could not be written"),
        }
    }
}

impl std::error::Error for FrameProblem {}

/// One message as a frame, ready to write. The buffer is made big enough
/// for the longest message at once, so that it never grows and leaves a
/// copy behind, and it is wiped when dropped.
pub fn encode<T: Serialize>(message: &T) -> Result<Frame, FrameProblem> {
    let mut frame: Frame = Zeroizing::new(Vec::with_capacity(HEADER_BYTES + MAX_MESSAGE_BYTES + 1));
    frame.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    frame.extend_from_slice(&[0; 4]);
    serde_json::to_writer(&mut *frame, message).map_err(|_| FrameProblem::CannotWrite)?;
    let length = frame.len() - HEADER_BYTES;
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameProblem::TooBig { bytes: length });
    }
    let said = u32::try_from(length).map_err(|_| FrameProblem::TooBig { bytes: length })?;
    frame[2..HEADER_BYTES].copy_from_slice(&said.to_le_bytes());
    Ok(frame)
}

/// Reads frames out of the bytes a pipe gives, in whatever pieces they come.
///
/// Every frame read is wiped from its buffer, and all of it when it is
/// dropped. After it returns a problem it must not be used again: the
/// connection is closed.
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
    /// An empty decoder. Its buffer is made at its full size once, so it
    /// never grows and leaves a copy behind.
    pub fn new() -> Decoder {
        Decoder { buffer: Zeroizing::new(Vec::with_capacity(MAX_BUFFERED)) }
    }

    /// Bytes as they came from the pipe. Refused when more is waiting than a
    /// connection may hold.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), FrameProblem> {
        let waiting = self.buffer.len().saturating_add(bytes.len());
        if waiting > MAX_BUFFERED {
            return Err(FrameProblem::TooBig { bytes: waiting });
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// The next whole message, if one is waiting. A frame of the wrong
    /// version or too long is refused from its header alone.
    pub fn next_message<T: DeserializeOwned>(&mut self) -> Result<Option<T>, FrameProblem> {
        if self.buffer.len() < HEADER_BYTES {
            return Ok(None);
        }
        let version = u16::from_le_bytes([self.buffer[0], self.buffer[1]]);
        if version != PROTOCOL_VERSION {
            return Err(FrameProblem::WrongVersion { got: version });
        }
        let said = u32::from_le_bytes([self.buffer[2], self.buffer[3], self.buffer[4], self.buffer[5]]);
        let length = usize::try_from(said).unwrap_or(usize::MAX);
        if length > MAX_MESSAGE_BYTES {
            return Err(FrameProblem::TooBig { bytes: length });
        }
        let end = HEADER_BYTES + length;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let parsed = serde_json::from_slice::<T>(&self.buffer[HEADER_BYTES..end]);
        self.consume(end);
        match parsed {
            Ok(message) => Ok(Some(message)),
            Err(e) => Err(FrameProblem::Malformed { line: e.line(), column: e.column() }),
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
    use serde_json::json;

    fn frame_of(json: &str) -> Vec<u8> {
        let mut bytes = PROTOCOL_VERSION.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    #[test]
    fn a_request_crosses_whole_and_in_pieces() {
        let sign_in = Request::SignIn { password: Secret::new("Mira's long sentence".to_string()) };
        let frame = encode(&sign_in).unwrap();
        assert_eq!(u16::from_le_bytes([frame[0], frame[1]]), PROTOCOL_VERSION);
        let mut decoder = Decoder::new();
        for piece in frame.chunks(3) {
            assert_eq!(decoder.next_message::<Request>().unwrap(), None);
            decoder.push(piece).unwrap();
        }
        assert_eq!(decoder.next_message::<Request>().unwrap(), Some(sign_in));
        assert_eq!(decoder.waiting(), 0);

        let two = [encode(&Request::About).unwrap().to_vec(), encode(&Answer::SignedIn).unwrap().to_vec()].concat();
        let mut decoder = Decoder::new();
        decoder.push(&two).unwrap();
        assert_eq!(decoder.next_message::<Request>().unwrap(), Some(Request::About));
        assert_eq!(decoder.next_message::<Answer>().unwrap(), Some(Answer::SignedIn));
    }

    #[test]
    fn the_wire_looks_as_the_service_will_read_it() {
        let text = String::from_utf8(encode(&Request::SignIn { password: Secret::new("x".into()) }).unwrap()[HEADER_BYTES..].to_vec()).unwrap();
        assert_eq!(text, r#"{"request":"sign_in","password":"x"}"#);
        let text = String::from_utf8(
            encode(&Answer::Refused { why: Refusal::ChildAccount { child: "Asha".into() } }).unwrap()[HEADER_BYTES..].to_vec(),
        )
        .unwrap();
        assert_eq!(text, r#"{"answer":"refused","why":{"refusal":"child_account","child":"Asha"}}"#);
    }

    #[test]
    fn a_password_never_shows_in_debug_output() {
        let request = Request::SignIn { password: Secret::new("Mira's long sentence".to_string()) };
        let shown = format!("{request:?}");
        assert!(!shown.contains("Mira"), "{shown}");
        assert!(shown.contains("Secret(hidden)"), "{shown}");
    }

    #[test]
    fn a_page_asks_by_name_and_only_for_what_exists() {
        assert_eq!(Request::from_page("about", &json!({})).unwrap(), Request::About);
        assert_eq!(Request::from_page("about", &serde_json::Value::Null).unwrap(), Request::About);
        assert_eq!(
            Request::from_page("sign_in", &json!({ "password": "pw" })).unwrap(),
            Request::SignIn { password: Secret::new("pw".into()) }
        );
        assert_eq!(Request::from_page("delete_family", &json!({})).unwrap_err(), "the console has no request named delete_family");
        let wrong = Request::from_page("sign_in", &json!({ "password": 5 })).unwrap_err();
        assert_eq!(wrong, "the arguments of sign_in are not the ones it takes");
        let extra = Request::from_page("sign_in", &json!({ "password": "secret words", "more": 1 })).unwrap_err();
        assert!(!extra.contains("secret words"), "{extra}");
        assert!(Request::from_page("about", &json!({ "request": "sign_in" })).is_err());
        assert!(Request::from_page("about", &json!([1])).is_err());
        assert_eq!(Request::SignIn { password: Secret::new("p".into()) }.name(), "sign_in");
    }

    #[test]
    fn a_wrong_version_or_too_long_a_frame_is_refused_from_its_header() {
        let mut bytes = 2u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        let mut decoder = Decoder::new();
        decoder.push(&bytes).unwrap();
        assert_eq!(decoder.next_message::<Request>(), Err(FrameProblem::WrongVersion { got: 2 }));

        let mut header = PROTOCOL_VERSION.to_le_bytes().to_vec();
        header.extend_from_slice(&((MAX_MESSAGE_BYTES as u32) + 1).to_le_bytes());
        let mut decoder = Decoder::new();
        decoder.push(&header).unwrap();
        assert_eq!(decoder.next_message::<Request>(), Err(FrameProblem::TooBig { bytes: MAX_MESSAGE_BYTES + 1 }));

        let mut decoder = Decoder::new();
        assert!(decoder.push(&vec![0u8; MAX_BUFFERED + 1]).is_err());
    }

    #[test]
    fn a_malformed_message_is_refused_without_quoting_it() {
        let mut decoder = Decoder::new();
        decoder.push(&frame_of(r#"{"request":"sign_in","password":"hunter two","x":1}"#)).unwrap();
        let problem = decoder.next_message::<Request>().unwrap_err();
        assert!(matches!(problem, FrameProblem::Malformed { .. }));
        assert!(!problem.to_string().contains("hunter"), "{problem}");

        let mut decoder = Decoder::new();
        decoder.push(&frame_of(r#"{"request":"open_everything"}"#)).unwrap();
        assert!(matches!(decoder.next_message::<Request>(), Err(FrameProblem::Malformed { .. })));
    }

    #[test]
    fn the_longest_message_fits_and_one_byte_more_does_not() {
        // A string of n characters is n + 2 bytes of JSON.
        let fits = "x".repeat(MAX_MESSAGE_BYTES - 2);
        let frame = encode(&fits).unwrap();
        assert_eq!(frame.len(), HEADER_BYTES + MAX_MESSAGE_BYTES);
        let mut decoder = Decoder::new();
        decoder.push(&frame).unwrap();
        assert_eq!(decoder.next_message::<String>().unwrap().map(|s| s.len()), Some(MAX_MESSAGE_BYTES - 2));
        let too_long = "x".repeat(MAX_MESSAGE_BYTES - 1);
        assert_eq!(encode(&too_long).unwrap_err(), FrameProblem::TooBig { bytes: MAX_MESSAGE_BYTES + 1 });
    }
}
