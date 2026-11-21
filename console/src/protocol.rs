//! The console's door: what the console and LiMo's service say to each
//! other, and how it travels. The service's side is LiMo for Windows'
//! `console` module (`../windows/README.md`, "The console's door"); this is
//! the console's, written to the same wire, and tested against the
//! service's own reader and writer (`real_door_tests.rs`).
//!
//! The door is the named pipe [`DOOR_PIPE`], which only the service makes.
//! Each message is a frame, as on LiMo for Windows' helper channel: the
//! protocol's version (2 bytes), the length of what follows (4 bytes), both
//! little endian, then that many bytes of JSON; a frame over
//! [`MAX_MESSAGE_BYTES`], or of another version, is refused from its header
//! alone.
//!
//! The console says hello first, `{"say":"hello","protocol":1}`, and is
//! answered `{"say":"hello","protocol":1,"version":"0.1.0"}`, or
//! `{"say":"refused","message":"..."}` and the connection is closed. Then
//! requests, each with a number of the console's own:
//! `{"say":"request","id":7,"request":{"name":"lock_now","args":{...}}}`,
//! answered `{"say":"answer","id":7,"reply":{...}}`, the reply `{"outcome":
//! "done","value":...}` or `{"outcome":"problem","problem":{"kind":...,
//! "message":...}}`.
//!
//! The console forwards a page's request by its name and its arguments, and
//! knows no request by heart: the service reads each, and answers one it
//! cannot read with `unknown_request`. What the console holds of a request
//! is wiped once its frame is written: every string in its arguments (a
//! password, a PIN), and the frame. No error about a frame quotes any part of
//! it.

#![forbid(unsafe_code)]

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroize::{Zeroize, Zeroizing};

/// The door's name. Only LiMo's service makes it; the console checks who
/// did before it says anything (see [`crate::door`]).
pub const DOOR_PIPE: &str = r"\\.\pipe\limo.console";

/// The version of the console's protocol. A frame of any other version
/// closes the connection.
pub const PROTOCOL_VERSION: u16 = 1;

/// The bytes in front of each message: the version and the length.
pub const HEADER_BYTES: usize = 6;

/// The longest message either way, in bytes of JSON: 1 MiB.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// The longest request name the console forwards.
pub const MAX_NAME_CHARS: usize = 40;

/// The most a [`Decoder`] ever holds: one whole frame and the start of the
/// next.
const MAX_BUFFERED: usize = 2 * (HEADER_BYTES + MAX_MESSAGE_BYTES);

/// One frame, ready to write. Wiped from memory when dropped.
pub type Frame = Zeroizing<Vec<u8>>;

/// The console's hello: the first message of a connection.
pub fn hello() -> Value {
    serde_json::json!({ "say": "hello", "protocol": PROTOCOL_VERSION })
}

/// A request as the page asks it: a name and its arguments, checked for
/// their shape only. The service decides what each name means.
#[derive(PartialEq)]
pub struct PageRequest {
    name: String,
    args: Option<Map<String, Value>>,
}

impl fmt::Debug for PageRequest {
    // Never the arguments: a password may be among them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageRequest").field("name", &self.name).finish_non_exhaustive()
    }
}

/// Requests only the console itself asks, never a page: `make_offer`, whose
/// answer holds a pairing offer's secret. The console hands that to the
/// phone over the cable and wipes it (see [`crate::phone`]); a page has no
/// use for it, so no page ever sees one.
pub const CONSOLE_ONLY: [&str; 1] = ["make_offer"];

impl PageRequest {
    /// The request a page asked for, or why there is none: as
    /// [`PageRequest::from_console`], and never one of [`CONSOLE_ONLY`].
    pub fn from_page(name: &str, mut args: Value) -> Result<PageRequest, String> {
        if CONSOLE_ONLY.contains(&name) {
            wipe(&mut args);
            return Err(format!("{name} is asked by the console itself, never by a page"));
        }
        PageRequest::from_console(name, args)
    }

    /// A request the console asks: a name of 1 to [`MAX_NAME_CHARS`] of `a`
    /// to `z` and `_`, and arguments that are an object, or nothing. Nothing
    /// about the arguments' values is ever quoted.
    pub fn from_console(name: &str, args: Value) -> Result<PageRequest, String> {
        let shaped = !name.is_empty()
            && name.chars().count() <= MAX_NAME_CHARS
            && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            && !name.starts_with('_');
        if !shaped {
            return Err("that is not the name of a request".to_string());
        }
        let args = match args {
            Value::Null => None,
            Value::Object(map) if map.is_empty() => None,
            Value::Object(map) => Some(map),
            mut other => {
                wipe(&mut other);
                return Err(format!("the arguments of {name} are not an object"));
            }
        };
        Ok(PageRequest { name: name.to_string(), args })
    }

    /// The request's name, for the log. Never its arguments.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The whole message, with the console's number for it, as a frame. The
    /// request's arguments are wiped as they are written, whatever happens.
    pub fn into_frame(mut self, id: u32) -> Result<Frame, FrameProblem> {
        let mut request = Map::new();
        request.insert("name".to_string(), Value::String(std::mem::take(&mut self.name)));
        if let Some(args) = self.args.take() {
            request.insert("args".to_string(), Value::Object(args));
        }
        let mut message = Value::Object(Map::from_iter([
            ("say".to_string(), Value::String("request".to_string())),
            ("id".to_string(), Value::from(id)),
            ("request".to_string(), Value::Object(request)),
        ]));
        let frame = encode(&message);
        wipe(&mut message);
        frame
    }
}

impl Drop for PageRequest {
    fn drop(&mut self) {
        if let Some(args) = self.args.as_mut() {
            for value in args.values_mut() {
                wipe(value);
            }
        }
    }
}

/// Wipes every string in a JSON value, at any depth, and every key.
pub fn wipe(value: &mut Value) {
    match value {
        Value::String(s) => s.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Object(map) => {
            let taken = std::mem::take(map);
            for (mut key, mut v) in taken {
                key.zeroize();
                wipe(&mut v);
            }
        }
        _ => {}
    }
}

/// What the service says to the console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "say", rename_all = "snake_case")]
pub enum FromDoor {
    /// The answer to hello.
    Hello {
        /// The protocol's version.
        protocol: u16,
        /// LiMo for Windows' version.
        version: String,
    },
    /// The door does not open for this connection, in a sentence for the
    /// guardian's screen; the connection is closed after it.
    Refused {
        /// Why.
        message: String,
    },
    /// The answer to a request.
    Answer {
        /// The console's number for the request.
        id: u32,
        /// What became of it: `{"outcome":"done","value":...}` or
        /// `{"outcome":"problem","problem":{...}}`, handed to the page as
        /// it came.
        reply: Value,
    },
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
                write!(f, "a message of {bytes} bytes, more than the {MAX_MESSAGE_BYTES} a message may have")
            }
            FrameProblem::Malformed { line, column } => write!(
                f,
                "a message the console cannot read (the JSON stops making sense at line {line}, column {column})"
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
/// Its buffer is made at its full size once, so it never grows and leaves a
/// copy behind; every frame read is wiped from it, and all of it when it is
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
    /// An empty decoder.
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

    fn body(frame: &[u8]) -> Value {
        serde_json::from_slice(&frame[HEADER_BYTES..]).unwrap()
    }

    #[test]
    fn a_page_s_request_goes_on_the_wire_as_the_service_reads_it() {
        let r = PageRequest::from_page("sign_in", json!({ "password": "Mira's long sentence" })).unwrap();
        assert_eq!(r.name(), "sign_in");
        let frame = r.into_frame(7).unwrap();
        assert_eq!(u16::from_le_bytes([frame[0], frame[1]]), PROTOCOL_VERSION);
        assert_eq!(
            body(&frame),
            json!({ "say": "request", "id": 7, "request": { "name": "sign_in", "args": { "password": "Mira's long sentence" } } })
        );
        // No arguments, or empty ones: none on the wire, as the service
        // reads a request that takes none.
        let today = PageRequest::from_page("family_today", json!({})).unwrap().into_frame(8).unwrap();
        assert_eq!(body(&today), json!({ "say": "request", "id": 8, "request": { "name": "family_today" } }));
        let about = PageRequest::from_page("about", Value::Null).unwrap().into_frame(9).unwrap();
        assert_eq!(body(&about), json!({ "say": "request", "id": 9, "request": { "name": "about" } }));
        assert_eq!(body(&encode(&hello()).unwrap()), json!({ "say": "hello", "protocol": 1 }));
    }

    #[test]
    fn only_a_name_of_the_right_shape_and_an_object_of_arguments_are_forwarded() {
        for bad in ["", "Sign_in", "sign-in", "sign in", "_x", "../x", "a.b", "é", &"a".repeat(41)] {
            assert!(PageRequest::from_page(bad, Value::Null).is_err(), "{bad:?}");
        }
        assert!(PageRequest::from_page(&"a".repeat(40), Value::Null).is_ok());
        let problem = PageRequest::from_page("sign_in", json!(["the secret words"])).unwrap_err();
        assert!(!problem.contains("secret"), "{problem}");
        assert!(PageRequest::from_page("sign_in", json!("x")).is_err());
    }

    #[test]
    fn a_page_never_asks_for_a_pairing_offer_and_the_console_may() {
        let refused = PageRequest::from_page("make_offer", json!({ "child": "c1" })).unwrap_err();
        assert_eq!(refused, "make_offer is asked by the console itself, never by a page");
        let made = PageRequest::from_console("make_offer", json!({ "child": "c1" })).unwrap().into_frame(4).unwrap();
        assert_eq!(body(&made), json!({ "say": "request", "id": 4, "request": { "name": "make_offer", "args": { "child": "c1" } } }));
    }

    #[test]
    fn a_password_never_shows_in_debug_output() {
        let r = PageRequest::from_page("sign_in", json!({ "password": "Mira's long sentence" })).unwrap();
        let shown = format!("{r:?}");
        assert!(shown.contains("sign_in") && !shown.contains("Mira"), "{shown}");
    }

    #[test]
    fn wiping_reaches_every_string_at_any_depth() {
        let mut v = json!({ "old": "one", "new": ["two", { "deep": "three" }], "n": 4 });
        wipe(&mut v);
        assert_eq!(v, json!({}));
        let mut s = json!("four");
        wipe(&mut s);
        assert_eq!(s, json!(""));
    }

    #[test]
    fn the_door_s_answers_read() {
        let mut decoder = Decoder::new();
        for json in [
            json!({ "say": "hello", "protocol": 1, "version": "0.1.0" }),
            json!({ "say": "refused", "message": "LiMo's settings cannot be opened from Asha's account. A guardian opens them from their own." }),
            json!({ "say": "answer", "id": 3, "reply": { "outcome": "problem", "problem": { "kind": "wrong_password", "message": "x", "tries_before_wait": 3 } } }),
        ] {
            decoder.push(&encode(&json).unwrap()).unwrap();
        }
        assert_eq!(decoder.next_message::<FromDoor>().unwrap(), Some(FromDoor::Hello { protocol: 1, version: "0.1.0".into() }));
        assert!(matches!(decoder.next_message::<FromDoor>().unwrap(), Some(FromDoor::Refused { .. })));
        match decoder.next_message::<FromDoor>().unwrap() {
            Some(FromDoor::Answer { id: 3, reply }) => assert_eq!(reply["problem"]["tries_before_wait"], 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_wrong_version_or_too_long_a_frame_is_refused_from_its_header() {
        let mut bytes = 2u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&3u32.to_le_bytes());
        let mut decoder = Decoder::new();
        decoder.push(&bytes).unwrap();
        assert_eq!(decoder.next_message::<FromDoor>(), Err(FrameProblem::WrongVersion { got: 2 }));

        let mut header = PROTOCOL_VERSION.to_le_bytes().to_vec();
        header.extend_from_slice(&((MAX_MESSAGE_BYTES as u32) + 1).to_le_bytes());
        let mut decoder = Decoder::new();
        decoder.push(&header).unwrap();
        assert_eq!(decoder.next_message::<FromDoor>(), Err(FrameProblem::TooBig { bytes: MAX_MESSAGE_BYTES + 1 }));

        let mut decoder = Decoder::new();
        assert!(decoder.push(&vec![0u8; MAX_BUFFERED + 1]).is_err());
    }

    #[test]
    fn a_message_that_is_not_the_door_s_is_refused_without_quoting_it() {
        let mut decoder = Decoder::new();
        decoder.push(&encode(&json!({ "say": "welcome", "secret": "hunter two" })).unwrap()).unwrap();
        let problem = decoder.next_message::<FromDoor>().unwrap_err();
        assert!(matches!(problem, FrameProblem::Malformed { .. }));
        assert!(!problem.to_string().contains("hunter"), "{problem}");
    }

    #[test]
    fn the_longest_message_fits_and_one_byte_more_does_not() {
        // A string of n characters is n + 2 bytes of JSON.
        let frame = encode(&"x".repeat(MAX_MESSAGE_BYTES - 2)).unwrap();
        assert_eq!(frame.len(), HEADER_BYTES + MAX_MESSAGE_BYTES);
        let mut decoder = Decoder::new();
        decoder.push(&frame).unwrap();
        assert_eq!(decoder.next_message::<String>().unwrap().map(|s| s.len()), Some(MAX_MESSAGE_BYTES - 2));
        assert_eq!(encode(&"x".repeat(MAX_MESSAGE_BYTES - 1)).unwrap_err(), FrameProblem::TooBig { bytes: MAX_MESSAGE_BYTES + 1 });
    }
}
