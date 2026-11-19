//! Frames, the preamble, and messages cut into pieces.
//!
//! Everything on the wire is a frame: a 4 byte big endian length, then that
//! many bytes. A frame's length is checked against what the step allows the
//! moment its 4 bytes are in, before any of its body is kept.
//!
//! - The phone's first frame is the preamble (in the clear) and the first
//!   handshake message. The preamble is `LiMo`, the link version (1 byte)
//!   and the kind of link (`P` for pairing, `S` for a session); a pairing
//!   preamble then has the offer's 8 byte id. The preamble is also the
//!   handshake's prologue, so a preamble changed on the way makes the
//!   handshake fail.
//! - Every other handshake frame is one handshake message, of the one size
//!   its step has.
//! - After the handshake, a frame carries one message of up to 1 MiB
//!   ([`MAX_MESSAGE_BYTES`]). A Noise message is at most 65535 bytes, so the
//!   message is cut into pieces of up to 65518 bytes; each piece is a flag
//!   byte (1 on the last piece, 0 on the others) and the text, encrypted as
//!   one Noise message of up to 65535 bytes. Every piece but the last is
//!   exactly 65535 bytes, so the frame's length alone says where each piece
//!   ends. A frame cut short at a piece boundary (and its length changed to
//!   match) ends on a piece whose flag is 0, and is refused: the Noise
//!   specification leaves it to the application to notice a truncated
//!   stream (section 13, "Session termination").

use snow::{HandshakeState, TransportState};
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::TAG_BYTES;
use crate::error::LinkError;
use crate::offer::{OFFER_ID_BYTES, OfferId};

/// The bytes in front of each frame: its length.
pub const LENGTH_BYTES: usize = 4;

/// The longest message, in bytes of JSON: 1 MiB. A busy report, with a few
/// hundred slices of use made at school, is well under 100 KiB.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// The longest Noise message, from the Noise specification, section 3.
pub const NOISE_MAX_BYTES: usize = 65535;

/// The most text one piece carries: a Noise message less its tag and the
/// flag byte.
pub const PIECE_TEXT_BYTES: usize = NOISE_MAX_BYTES - TAG_BYTES - 1;

/// The most pieces a message can need.
pub const MAX_PIECES: usize = MAX_MESSAGE_BYTES.div_ceil(PIECE_TEXT_BYTES);

/// The longest frame body after the handshake: the longest message, and a
/// tag and a flag for each of its pieces.
pub const MAX_FRAME_BYTES: usize = MAX_MESSAGE_BYTES + MAX_PIECES * (TAG_BYTES + 1);

/// The shortest frame body after the handshake: one piece with no text.
pub const MIN_FRAME_BYTES: usize = TAG_BYTES + 1;

/// The four bytes every preamble starts with.
pub const MAGIC: &[u8; 4] = b"LiMo";

/// The preamble of a session: `LiMo`, the version, `S`.
pub const SESSION_PREAMBLE_BYTES: usize = 6;
/// The preamble of a pairing: `LiMo`, the version, `P`, the offer's id.
pub const PAIRING_PREAMBLE_BYTES: usize = 6 + OFFER_ID_BYTES;

/// Which link a first frame starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Pairing(OfferId),
    Session,
}

/// The preamble a phone sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Preamble {
    pub version: u8,
    pub kind: Kind,
}

impl Preamble {
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PAIRING_PREAMBLE_BYTES);
        out.extend_from_slice(MAGIC);
        out.push(self.version);
        match self.kind {
            Kind::Pairing(id) => {
                out.push(b'P');
                out.extend_from_slice(id.as_bytes());
            }
            Kind::Session => out.push(b'S'),
        }
        out
    }

    /// Reads the preamble at the start of a first frame's body, and returns
    /// it with the handshake message after it.
    pub fn read(body: &[u8]) -> Result<(Preamble, &[u8]), LinkError> {
        if body.len() < SESSION_PREAMBLE_BYTES || &body[..4] != MAGIC {
            return Err(LinkError::NotLiMo);
        }
        let version = body[4];
        if version == 0 {
            return Err(LinkError::BadVersion);
        }
        match body[5] {
            b'S' => Ok((Preamble { version, kind: Kind::Session }, &body[SESSION_PREAMBLE_BYTES..])),
            b'P' if body.len() >= PAIRING_PREAMBLE_BYTES => {
                let mut id = [0u8; OFFER_ID_BYTES];
                id.copy_from_slice(&body[6..PAIRING_PREAMBLE_BYTES]);
                Ok((Preamble { version, kind: Kind::Pairing(OfferId::from_bytes(id)) }, &body[PAIRING_PREAMBLE_BYTES..]))
            }
            _ => Err(LinkError::NotLiMo),
        }
    }
}

/// A frame: the length, then the body.
pub(crate) fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(LENGTH_BYTES + body.len());
    // Every body this crate makes is at most MAX_FRAME_BYTES, far below
    // what four bytes hold.
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// What a frame's length may be at this step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Allowed {
    /// Exactly one of these lengths (a handshake step).
    Exactly(&'static str, &'static [usize]),
    /// A message after the handshake.
    Message,
}

impl Allowed {
    fn check(self, said: usize) -> Result<(), LinkError> {
        match self {
            Allowed::Exactly(step, sizes) => {
                if sizes.contains(&said) {
                    Ok(())
                } else if said > sizes.iter().copied().max().unwrap_or(0) {
                    Err(LinkError::FrameTooLong { said, most: sizes.iter().copied().max().unwrap_or(0) })
                } else {
                    Err(LinkError::WrongSize { step, said, expected: sizes.first().copied().unwrap_or(0) })
                }
            }
            Allowed::Message => {
                if said > MAX_FRAME_BYTES {
                    Err(LinkError::FrameTooLong { said, most: MAX_FRAME_BYTES })
                } else if pieces_in(said).is_none() {
                    Err(LinkError::BadLength { said })
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// How many pieces a frame body of this length holds, if it can hold
/// whole pieces at all, and the text they carry stays within a message.
pub(crate) fn pieces_in(len: usize) -> Option<usize> {
    if len < MIN_FRAME_BYTES {
        return None;
    }
    let full = len / NOISE_MAX_BYTES;
    let rest = len % NOISE_MAX_BYTES;
    let pieces = match rest {
        0 => full,
        r if r >= MIN_FRAME_BYTES => full + 1,
        _ => return None,
    };
    let text = len - pieces * (TAG_BYTES + 1);
    (pieces <= MAX_PIECES && text <= MAX_MESSAGE_BYTES).then_some(pieces)
}

/// Gathers one frame at a time out of the bytes a socket gives, in whatever
/// pieces they come. It never holds more than the frame the step allows.
#[derive(Debug, Default)]
pub(crate) struct FrameReader {
    head: [u8; LENGTH_BYTES],
    head_len: usize,
    body: Vec<u8>,
    want: Option<usize>,
}

impl FrameReader {
    /// Takes from `input` what the current frame still needs. Returns its
    /// body once it is whole, leaving the rest of `input` for the next
    /// frame. A length the step does not allow is refused at once.
    pub fn feed(&mut self, input: &mut &[u8], allowed: Allowed) -> Result<Option<Vec<u8>>, LinkError> {
        while self.head_len < LENGTH_BYTES {
            let Some((&byte, rest)) = input.split_first() else {
                return Ok(None);
            };
            self.head[self.head_len] = byte;
            self.head_len += 1;
            *input = rest;
        }
        let want = match self.want {
            Some(want) => want,
            None => {
                let said = usize::try_from(u32::from_be_bytes(self.head)).unwrap_or(usize::MAX);
                allowed.check(said)?;
                self.want = Some(said);
                said
            }
        };
        let take = (want - self.body.len()).min(input.len());
        self.body.extend_from_slice(&input[..take]);
        *input = &input[take..];
        if self.body.len() < want {
            return Ok(None);
        }
        self.head_len = 0;
        self.want = None;
        Ok(Some(std::mem::take(&mut self.body)))
    }

    /// Bytes of a frame not yet whole.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.head_len + self.body.len()
    }
}

/// Writes one handshake message into a body.
pub(crate) fn handshake_write(hs: &mut HandshakeState, size: usize) -> Result<Vec<u8>, LinkError> {
    let mut out = vec![0u8; size];
    let n = hs.write_message(&[], &mut out).map_err(|_| LinkError::Internal { what: "write a handshake message" })?;
    if n != size {
        return Err(LinkError::Internal { what: "write a handshake message of the expected size" });
    }
    Ok(out)
}

/// Reads one handshake message. Its payload must be empty.
pub(crate) fn handshake_read(hs: &mut HandshakeState, message: &[u8]) -> Result<(), LinkError> {
    let mut payload = Zeroizing::new(vec![0u8; message.len()]);
    match hs.read_message(message, &mut payload) {
        Ok(0) => Ok(()),
        Ok(_) => Err(LinkError::Unexpected { what: "a handshake message with a payload" }),
        Err(_) => Err(LinkError::Garbled),
    }
}

/// Encrypts one message as a frame body cut into pieces.
pub(crate) fn seal(noise: &mut TransportState, text: &[u8]) -> Result<Vec<u8>, LinkError> {
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(LinkError::MessageTooLong { bytes: text.len() });
    }
    let chunks: Vec<&[u8]> = if text.is_empty() { vec![&[][..]] } else { text.chunks(PIECE_TEXT_BYTES).collect() };
    let body_len = text.len() + chunks.len() * (TAG_BYTES + 1);
    let mut body = vec![0u8; body_len];
    let mut at = 0;
    let last = chunks.len() - 1;
    for (i, chunk) in chunks.iter().enumerate() {
        let mut piece = Zeroizing::new(Vec::with_capacity(chunk.len() + 1));
        piece.push(u8::from(i == last));
        piece.extend_from_slice(chunk);
        let n = noise
            .write_message(&piece, &mut body[at..])
            .map_err(|_| LinkError::Internal { what: "encrypt a message" })?;
        if n != piece.len() + TAG_BYTES {
            return Err(LinkError::Internal { what: "encrypt a message to the expected size" });
        }
        at += n;
    }
    Ok(body)
}

/// Decrypts a frame body back into one message. The text is in memory
/// that is wiped when dropped, and was never moved or grown.
pub(crate) fn open(noise: &mut TransportState, body: &[u8]) -> Result<Zeroizing<Vec<u8>>, LinkError> {
    let pieces = pieces_in(body.len()).ok_or(LinkError::BadLength { said: body.len() })?;
    let text_len = body.len() - pieces * (TAG_BYTES + 1);
    // Room for the text and one flag byte, made once.
    let mut text = Zeroizing::new(Vec::with_capacity(text_len + 1));
    for (i, piece) in body.chunks(NOISE_MAX_BYTES).enumerate() {
        let at = text.len();
        text.resize(at + piece.len() - TAG_BYTES, 0);
        let n = match noise.read_message(piece, &mut text[at..]) {
            Ok(n) => n,
            Err(_) => return Err(LinkError::Garbled),
        };
        let flag = text[at];
        let wanted = u8::from(i + 1 == pieces);
        if n != piece.len() - TAG_BYTES || flag != wanted {
            return Err(LinkError::Garbled);
        }
        text.copy_within(at + 1..at + n, at);
        let end = at + n - 1;
        text[end..].zeroize();
        text.truncate(end);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_add_up() {
        assert_eq!(PIECE_TEXT_BYTES, 65518);
        assert_eq!(MAX_PIECES, 17);
        assert_eq!(MAX_FRAME_BYTES, 1_048_576 + 17 * 17);
        assert!(u32::try_from(MAX_FRAME_BYTES).is_ok());
        // A message of exactly the longest length fits.
        assert_eq!(pieces_in(MAX_MESSAGE_BYTES + MAX_PIECES * 17), Some(MAX_PIECES));
    }

    #[test]
    fn piece_counts_from_lengths() {
        assert_eq!(pieces_in(0), None);
        assert_eq!(pieces_in(16), None);
        assert_eq!(pieces_in(17), Some(1));
        assert_eq!(pieces_in(65535), Some(1));
        // A whole piece and a scrap too short to be a piece.
        assert_eq!(pieces_in(65535 + 16), None);
        assert_eq!(pieces_in(65535 + 17), Some(2));
        assert_eq!(pieces_in(2 * 65535), Some(2));
        assert_eq!(pieces_in(MAX_FRAME_BYTES), Some(MAX_PIECES));
        // One byte more is one byte of text more than a message may have.
        assert_eq!(pieces_in(MAX_FRAME_BYTES + 1), None);
        assert_eq!(pieces_in(18 * 65535), None);
        assert_eq!(pieces_in(usize::MAX), None);
    }

    #[test]
    fn a_frame_too_long_is_refused_from_its_length_alone() {
        let mut reader = FrameReader::default();
        let mut input: &[u8] = &u32::MAX.to_be_bytes();
        assert_eq!(
            reader.feed(&mut input, Allowed::Message),
            Err(LinkError::FrameTooLong { said: u32::MAX as usize, most: MAX_FRAME_BYTES })
        );
        let mut reader = FrameReader::default();
        let mut input: &[u8] = &[0, 0, 0, 63];
        assert_eq!(
            reader.feed(&mut input, Allowed::Exactly("first frame", &[54, 62])),
            Err(LinkError::FrameTooLong { said: 63, most: 62 })
        );
        let mut reader = FrameReader::default();
        let mut input: &[u8] = &[0, 0, 0, 55];
        assert_eq!(
            reader.feed(&mut input, Allowed::Exactly("first frame", &[54, 62])),
            Err(LinkError::WrongSize { step: "first frame", said: 55, expected: 54 })
        );
    }

    #[test]
    fn frames_come_whole_from_any_pieces() {
        let mut all = frame(b"hello");
        all.extend(frame(b"there!"));
        let mut reader = FrameReader::default();
        let mut got = Vec::new();
        for byte in &all {
            let mut input: &[u8] = std::slice::from_ref(byte);
            if let Some(body) = reader.feed(&mut input, Allowed::Exactly("t", &[5, 6])).unwrap() {
                got.push(body);
            }
            assert!(input.is_empty());
        }
        assert_eq!(got, vec![b"hello".to_vec(), b"there!".to_vec()]);
        let mut reader = FrameReader::default();
        let mut input: &[u8] = &all;
        assert_eq!(reader.feed(&mut input, Allowed::Exactly("t", &[5, 6])).unwrap(), Some(b"hello".to_vec()));
        assert_eq!(input.len(), LENGTH_BYTES + 6);
        assert_eq!(reader.feed(&mut input, Allowed::Exactly("t", &[5, 6])).unwrap(), Some(b"there!".to_vec()));
        assert_eq!(reader.waiting(), 0);
    }

    #[test]
    fn preambles_both_ways() {
        let id = OfferId::from_bytes([9; 8]);
        for p in [Preamble { version: 1, kind: Kind::Session }, Preamble { version: 7, kind: Kind::Pairing(id) }] {
            let mut body = p.bytes();
            body.extend_from_slice(b"rest");
            let (back, rest) = Preamble::read(&body).unwrap();
            assert_eq!(back, p);
            assert_eq!(rest, b"rest");
        }
        assert_eq!(Preamble::read(b"LiMo\x01"), Err(LinkError::NotLiMo));
        assert_eq!(Preamble::read(b"LiMa\x01S"), Err(LinkError::NotLiMo));
        assert_eq!(Preamble::read(b"LiMo\x00S"), Err(LinkError::BadVersion));
        assert_eq!(Preamble::read(b"LiMo\x01X"), Err(LinkError::NotLiMo));
        assert_eq!(Preamble::read(b"LiMo\x01P1234567"), Err(LinkError::NotLiMo));
    }
}
