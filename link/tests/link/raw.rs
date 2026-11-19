//! A phone written straight on `snow` and its own resolver, from the
//! README's description of the wire and nothing of this crate's: so that
//! the master can be sent what a well behaved phone never sends (another
//! version, a report about another device, JSON of any shape), and so that
//! the README's description is itself checked against the code.

use snow::params::NoiseParams;
use snow::resolvers::{DefaultResolver, FallbackResolver};
use snow::types::Random;
use snow::{Builder, HandshakeState, TransportState};

use crate::common::Rng;

/// Random numbers for snow from the stories' seeded generator; the stories
/// fix every ephemeral key from it anyway.
struct SeededRandom(Rng);

impl Random for SeededRandom {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), snow::Error> {
        dest.copy_from_slice(&self.0.bytes(dest.len()));
        Ok(())
    }
}

struct Resolver(u64);

impl snow::resolvers::CryptoResolver for Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        Some(Box::new(SeededRandom(Rng::new(self.0))))
    }
    fn resolve_dh(&self, _: &snow::params::DHChoice) -> Option<Box<dyn snow::types::Dh>> {
        None
    }
    fn resolve_hash(&self, _: &snow::params::HashChoice) -> Option<Box<dyn snow::types::Hash>> {
        None
    }
    fn resolve_cipher(&self, _: &snow::params::CipherChoice) -> Option<Box<dyn snow::types::Cipher>> {
        None
    }
}

fn builder<'a>(protocol: &str, seed: u64) -> Builder<'a> {
    let params: NoiseParams = protocol.parse().unwrap();
    Builder::with_resolver(params, Box::new(FallbackResolver::new(Box::new(DefaultResolver), Box::new(Resolver(seed)))))
}

/// A frame: 4 bytes of length, big endian, then the body.
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

/// The body of a frame.
pub fn body(frame: &[u8]) -> &[u8] {
    let len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
    assert_eq!(frame.len(), 4 + len);
    &frame[4..]
}

/// A phone's half of a handshake, made by hand.
pub struct RawHandshake {
    pub hs: HandshakeState,
}

impl RawHandshake {
    /// A session (KK) handshake with this phone key and master key, and a
    /// preamble of the given version. Returns it and the first frame.
    pub fn session(phone_private: &[u8; 32], master_public: &[u8; 32], version: u8, seed: u64) -> (RawHandshake, Vec<u8>) {
        let preamble = [b'L', b'i', b'M', b'o', version, b'S'];
        let mut hs = builder("Noise_KK_25519_ChaChaPoly_BLAKE2s", seed)
            .local_private_key(phone_private)
            .unwrap()
            .remote_public_key(master_public)
            .unwrap()
            .prologue(&preamble)
            .unwrap()
            .build_initiator()
            .unwrap();
        let mut message = [0u8; 128];
        let n = hs.write_message(&[], &mut message).unwrap();
        let mut first = preamble.to_vec();
        first.extend_from_slice(&message[..n]);
        (RawHandshake { hs }, frame(&first))
    }

    /// A pairing (XXpsk3) handshake with this phone key, the offer's id and
    /// secret, and a preamble of the given version.
    pub fn pairing(phone_private: &[u8; 32], offer_id: &[u8; 8], secret: &[u8; 32], version: u8, seed: u64) -> (RawHandshake, Vec<u8>) {
        let mut preamble = vec![b'L', b'i', b'M', b'o', version, b'P'];
        preamble.extend_from_slice(offer_id);
        let mut hs = builder("Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s", seed)
            .local_private_key(phone_private)
            .unwrap()
            .psk(3, secret)
            .unwrap()
            .prologue(&preamble)
            .unwrap()
            .build_initiator()
            .unwrap();
        let mut message = [0u8; 128];
        let n = hs.write_message(&[], &mut message).unwrap();
        preamble.extend_from_slice(&message[..n]);
        (RawHandshake { hs }, frame(&preamble))
    }

    /// Reads the master's reply; for pairing, returns the finish frame too.
    pub fn reply(&mut self, reply: &[u8]) -> Result<Option<Vec<u8>>, snow::Error> {
        let mut payload = [0u8; 256];
        self.hs.read_message(body(reply), &mut payload)?;
        if self.hs.is_handshake_finished() {
            return Ok(None);
        }
        let mut message = [0u8; 256];
        let n = self.hs.write_message(&[], &mut message)?;
        Ok(Some(frame(&message[..n])))
    }

    pub fn open(self) -> RawSession {
        RawSession { noise: self.hs.into_transport_mode().unwrap() }
    }
}

/// A phone's half of an open link, made by hand.
pub struct RawSession {
    pub noise: TransportState,
}

impl RawSession {
    /// One message as a frame: pieces of up to 65518 bytes, each a flag
    /// byte (1 on the last) and the text, each encrypted on its own.
    pub fn seal(&mut self, text: &[u8]) -> Vec<u8> {
        let chunks: Vec<&[u8]> = if text.is_empty() { vec![&[][..]] } else { text.chunks(65518).collect() };
        let mut body = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            let mut piece = vec![u8::from(i + 1 == chunks.len())];
            piece.extend_from_slice(chunk);
            let mut out = vec![0u8; piece.len() + 16];
            let n = self.noise.write_message(&piece, &mut out).unwrap();
            body.extend_from_slice(&out[..n]);
        }
        frame(&body)
    }

    /// One piece with the flag given, as a whole frame: for frames that lie
    /// about where they end.
    pub fn seal_piece(&mut self, flag: u8, text: &[u8]) -> Vec<u8> {
        let mut piece = vec![flag];
        piece.extend_from_slice(text);
        let mut out = vec![0u8; piece.len() + 16];
        let n = self.noise.write_message(&piece, &mut out).unwrap();
        frame(&out[..n])
    }

    /// Reads one frame of the master's back into its text.
    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, snow::Error> {
        let mut text = Vec::new();
        let body = body(frame);
        let pieces: Vec<&[u8]> = body.chunks(65535).collect();
        for (i, piece) in pieces.iter().enumerate() {
            let mut out = vec![0u8; piece.len()];
            let n = self.noise.read_message(piece, &mut out)?;
            assert_eq!(out[0], u8::from(i + 1 == pieces.len()));
            text.extend_from_slice(&out[1..n]);
        }
        Ok(text)
    }
}

/// Base32 as RFC 4648 has it, without padding, for reading an offer's
/// bytes in the stories.
pub fn unbase32(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => panic!("not base32"),
        };
        acc = (acc << 5) | u32::from(v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

pub fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// An offer's bytes, read from its text form by the README's table.
pub struct OfferBytes(pub Vec<u8>);

impl OfferBytes {
    pub fn of(text: &str) -> OfferBytes {
        OfferBytes(unbase32(text.strip_prefix("limo1:").unwrap()))
    }
    pub fn master_key(&self) -> [u8; 32] {
        self.0[..32].try_into().unwrap()
    }
    pub fn id(&self) -> [u8; 8] {
        self.0[32..40].try_into().unwrap()
    }
    pub fn secret(&self) -> [u8; 32] {
        self.0[40..72].try_into().unwrap()
    }
    pub fn text(&self) -> String {
        format!("limo1:{}", base32(&self.0))
    }
}

/// The master's half of a session, made by hand: reads a phone's first
/// frame with the master's private key and the phone's public key, and
/// returns the open session and the reply frame.
pub fn raw_master(master_private: &[u8; 32], phone_public: &[u8; 32], first: &[u8], seed: u64) -> (RawSession, Vec<u8>) {
    let first = body(first);
    let (preamble, message) = first.split_at(6);
    let mut hs = builder("Noise_KK_25519_ChaChaPoly_BLAKE2s", seed)
        .local_private_key(master_private)
        .unwrap()
        .remote_public_key(phone_public)
        .unwrap()
        .prologue(preamble)
        .unwrap()
        .build_responder()
        .unwrap();
    let mut payload = [0u8; 128];
    hs.read_message(message, &mut payload).unwrap();
    let mut reply = [0u8; 128];
    let n = hs.write_message(&[], &mut reply).unwrap();
    (RawSession { noise: hs.into_transport_mode().unwrap() }, frame(&reply[..n]))
}
