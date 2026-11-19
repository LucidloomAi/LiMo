//! Why a link ended, or why a call was refused, in plain words.
//!
//! No variant holds any byte that came in, and none holds a key or a
//! secret, so an error can go to a log as it is.

use std::fmt;

use crate::messages::Refusal;
use crate::offer::OfferProblem;

/// Why a link ended, or why a call was refused.
///
/// The rule is by call, not by error. An error from `receive` or `tick`
/// (bytes that came in, or the time) ends the link: every later call on it
/// returns [`LinkError::Closed`]. An error from a call that sends (sending
/// before the link is open, a message too long to send, a report or a grant
/// for another device, a grant with no report to answer, too many questions
/// at once) was refused before anything was written: it changes nothing,
/// and the link goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// A frame said it was longer than this step allows. Refused from its
    /// length alone, before any of it was kept.
    FrameTooLong {
        /// The length it said.
        said: usize,
        /// The most this step allows.
        most: usize,
    },
    /// A handshake frame said a length other than the one its step has.
    /// Refused from its length alone.
    WrongSize {
        /// Which step.
        step: &'static str,
        /// The length it said.
        said: usize,
        /// The length that step has (the shorter of the two, for a first
        /// frame, which is a session's or a pairing's).
        expected: usize,
    },
    /// A frame of a length that cannot be a whole number of pieces.
    BadLength {
        /// The length it said.
        said: usize,
    },
    /// The first frame is not the start of a LiMo link.
    NotLiMo,
    /// The first frame says it is version 0 of the link, which no LiMo
    /// speaks.
    BadVersion,
    /// Something did not decrypt, or a handshake did not check out: a wrong
    /// key, a wrong offer secret, or bytes that were changed, replayed,
    /// reordered or cut short on the way.
    Garbled,
    /// A pairing for an offer this master does not have: never made,
    /// forgotten, or made by another master.
    UnknownOffer,
    /// A session from a phone this master does not know.
    UnknownPhone,
    /// While pairing, the PC that answered was not the one the offer names.
    WrongMaster,
    /// A message that cannot be read. Only where the reading stopped is
    /// kept, never what was there.
    Malformed {
        /// The line in the JSON.
        line: usize,
        /// The column in that line.
        column: usize,
    },
    /// A message that may not come now, such as a report on a pairing link
    /// or a grant nobody asked for.
    Unexpected {
        /// What came, in plain words.
        what: &'static str,
    },
    /// A message with a value out of bounds, such as an empty name or a
    /// code secret of the wrong length.
    OutOfBounds {
        /// Which value.
        what: &'static str,
    },
    /// A report or a grant for another device than the one this session is
    /// with.
    OtherDevice,
    /// The master refused, for this reason.
    Refused(Refusal),
    /// The handshake did not finish in time.
    HandshakeTooSlow,
    /// A question went unanswered for too long. When a report was among the
    /// questions, the phone should take it that the master cannot be
    /// reached (`DeviceState::lost_contact`).
    NoAnswer {
        /// True when a report was waiting for its grant.
        report_waiting: bool,
    },
    /// Nothing came from the phone for too long.
    Silent,
    /// Too many questions are waiting for answers already.
    TooManyWaiting,
    /// A message to send is longer than a message may be.
    MessageTooLong {
        /// Its length in bytes.
        bytes: usize,
    },
    /// The link is not open yet: the handshake has not finished.
    NotOpen,
    /// The link has ended or failed, and takes nothing more. Start a new
    /// one.
    Closed,
    /// The operating system gave no random bytes.
    NoRandomness,
    /// A pairing offer that cannot be used.
    BadOffer(OfferProblem),
    /// The crypto library refused something this crate asked of it. A bug
    /// in this crate, never something the other side sent.
    Internal {
        /// What was asked, in plain words.
        what: &'static str,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::FrameTooLong { said, most } => {
                write!(f, "a frame said it was {said} bytes long, and this step allows at most {most}")
            }
            LinkError::WrongSize { step, said, .. } => {
                write!(f, "the {step} said it was {said} bytes long, which is not a size it can have")
            }
            LinkError::BadLength { said } => {
                write!(f, "a frame said it was {said} bytes long, which no message can be")
            }
            LinkError::NotLiMo => write!(f, "what came is not the start of a LiMo link"),
            LinkError::BadVersion => write!(f, "the other side said it speaks version 0 of the link"),
            LinkError::Garbled => write!(
                f,
                "something did not decrypt: a wrong key or secret, or bytes changed, replayed, reordered or cut short on the way"
            ),
            LinkError::UnknownOffer => write!(f, "a phone tried to pair with an offer this PC does not have"),
            LinkError::UnknownPhone => write!(f, "a phone this PC does not know tried to connect"),
            LinkError::WrongMaster => write!(f, "the PC that answered is not the one the pairing offer names"),
            LinkError::Malformed { line, column } => write!(
                f,
                "a message could not be read (the JSON stops making sense at line {line}, column {column})"
            ),
            LinkError::Unexpected { what } => write!(f, "{what} came when it may not"),
            LinkError::OutOfBounds { what } => write!(f, "a message had {what}"),
            LinkError::OtherDevice => write!(f, "a report or a grant was for another device than this link's"),
            LinkError::Refused(why) => write!(f, "the master PC refused: {}", why.sentence()),
            LinkError::HandshakeTooSlow => write!(f, "the handshake did not finish in time"),
            LinkError::NoAnswer { report_waiting: true } => {
                write!(f, "a report went unanswered for too long, so the master PC cannot be reached")
            }
            LinkError::NoAnswer { report_waiting: false } => write!(f, "a question went unanswered for too long"),
            LinkError::Silent => write!(f, "nothing came from the phone for too long"),
            LinkError::TooManyWaiting => write!(f, "too many messages are waiting for answers already"),
            LinkError::MessageTooLong { bytes } => {
                write!(f, "a message of {bytes} bytes is longer than a message may be")
            }
            LinkError::NotOpen => write!(f, "the link is not open yet"),
            LinkError::Closed => write!(f, "the link has ended; start a new one"),
            LinkError::NoRandomness => write!(f, "the operating system gave no random bytes"),
            LinkError::BadOffer(problem) => write!(f, "the pairing offer cannot be used: {problem}"),
            LinkError::Internal { what } => write!(f, "the crypto library refused to {what}"),
        }
    }
}

impl std::error::Error for LinkError {}

impl From<OfferProblem> for LinkError {
    fn from(problem: OfferProblem) -> LinkError {
        LinkError::BadOffer(problem)
    }
}
