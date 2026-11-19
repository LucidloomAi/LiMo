//! The Noise handshakes, and the crypto under them.
//!
//! The link runs two Noise protocols, both on X25519, ChaCha20 with
//! Poly1305, and BLAKE2s:
//!
//! - pairing: `Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s`, with the offer's 32
//!   byte secret as the pre-shared key;
//! - every session after it: `Noise_KK_25519_ChaChaPoly_BLAKE2s`, with both
//!   static keys known to both sides in advance.
//!
//! The `snow` crate runs the handshakes. Its primitives come from the
//! resolver here, over the same pure Rust crates snow's own default resolver
//! uses, with one difference that is the reason for it: every private key
//! (static and ephemeral), every cipher key and every hash state is wiped
//! from memory when it is dropped. snow's own resolver never wipes them. The
//! test `the_resolver_matches_snows_own_byte_for_byte` runs the same
//! handshakes and messages through both, with the same keys, and checks every
//! byte is the same.
//!
//! What snow keeps itself, and never wipes: the chaining key and the
//! handshake hash while a handshake runs, its own copy of the pre-shared key,
//! and the temporary copies it and the crypto crates make on the stack.

use blake2::{Blake2s256, Digest};
use chacha20poly1305::{AeadInOut, ChaCha20Poly1305, KeyInit};
use curve25519_dalek::montgomery::MontgomeryPoint;
use snow::params::{CipherChoice, DHChoice, HashChoice, NoiseParams};
use snow::resolvers::CryptoResolver;
use snow::types::{Cipher, Dh, Hash, Random};
use snow::{Builder, HandshakeState};
use zeroize::{Zeroize, Zeroizing};

use crate::error::LinkError;
use crate::keys::{KEY_BYTES, KeyPair, PublicKey};

/// The Noise protocol of pairing.
pub const PAIRING_PROTOCOL: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";
/// The Noise protocol of every session after pairing.
pub const SESSION_PROTOCOL: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";

/// The authentication tag ChaChaPoly adds to everything it encrypts.
pub(crate) const TAG_BYTES: usize = 16;

/// The pairing handshake's first message, phone to master (`e`), with an
/// empty payload: the ephemeral key, and the payload's tag (in a handshake
/// with a pre-shared key, the ephemeral key is mixed into the cipher key at
/// once, so even the first, empty payload is encrypted).
pub const PAIR_HELLO_BYTES: usize = KEY_BYTES + TAG_BYTES;
/// The pairing handshake's second message, master to phone (`e, ee, s,
/// es`): its ephemeral key, its static key encrypted, and the empty
/// payload's tag.
pub const PAIR_REPLY_BYTES: usize = KEY_BYTES + KEY_BYTES + TAG_BYTES + TAG_BYTES;
/// The pairing handshake's third message, phone to master (`s, se, psk`):
/// the phone's static key encrypted, and the empty payload's tag.
pub const PAIR_FINISH_BYTES: usize = KEY_BYTES + TAG_BYTES + TAG_BYTES;
/// A session handshake's first message, phone to master (`e, es, ss`).
pub const SESSION_HELLO_BYTES: usize = KEY_BYTES + TAG_BYTES;
/// A session handshake's second message, master to phone (`e, ee, se`).
pub const SESSION_REPLY_BYTES: usize = KEY_BYTES + TAG_BYTES;

/// The operating system's random numbers.
struct OsRandom;

impl Random for OsRandom {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), snow::Error> {
        getrandom::fill(dest).map_err(|_| snow::Error::Rng)
    }
}

/// X25519, with its private key wiped when dropped.
#[derive(Default)]
struct X25519 {
    private: Zeroizing<[u8; KEY_BYTES]>,
    public: [u8; KEY_BYTES],
}

impl X25519 {
    fn derive_public(&mut self) {
        let mut point = MontgomeryPoint::mul_base_clamped(*self.private);
        self.public = point.to_bytes();
        point.zeroize();
    }
}

impl Dh for X25519 {
    fn name(&self) -> &'static str {
        "25519"
    }

    fn pub_len(&self) -> usize {
        KEY_BYTES
    }

    fn priv_len(&self) -> usize {
        KEY_BYTES
    }

    fn set(&mut self, private: &[u8]) {
        // snow hands over exactly `priv_len` bytes: this crate's keys, or
        // ones it generated itself. Anything shorter leaves zeros.
        let n = private.len().min(KEY_BYTES);
        self.private.fill(0);
        self.private[..n].copy_from_slice(&private[..n]);
        self.derive_public();
    }

    fn generate(&mut self, rng: &mut dyn Random) -> Result<(), snow::Error> {
        rng.try_fill_bytes(&mut self.private[..])?;
        self.derive_public();
        Ok(())
    }

    fn pubkey(&self) -> &[u8] {
        &self.public
    }

    fn privkey(&self) -> &[u8] {
        &self.private[..]
    }

    fn dh(&self, public: &[u8], out: &mut [u8]) -> Result<(), snow::Error> {
        // As the Noise specification asks of the 25519 functions (section
        // 12.1): an invalid public key gives an output of all zeros, not an
        // error.
        let Ok(theirs) = <[u8; KEY_BYTES]>::try_from(public.get(..KEY_BYTES).ok_or(snow::Error::Dh)?) else {
            return Err(snow::Error::Dh);
        };
        let target = out.get_mut(..KEY_BYTES).ok_or(snow::Error::Dh)?;
        let mut shared = MontgomeryPoint(theirs).mul_clamped(*self.private);
        target.copy_from_slice(shared.as_bytes());
        shared.zeroize();
        Ok(())
    }
}

/// ChaCha20 with Poly1305, as Noise uses it (section 12.3): the nonce is 32
/// bits of zeros, then the counter as 64 bits little endian. The key is
/// wiped when dropped (the cipher crate's `zeroize` feature).
#[derive(Default)]
struct ChaChaPoly {
    cipher: Option<ChaCha20Poly1305>,
}

fn nonce_bytes(n: u64) -> chacha20poly1305::Nonce {
    let mut bytes = [0u8; 12];
    bytes[4..].copy_from_slice(&n.to_le_bytes());
    chacha20poly1305::Nonce::from(bytes)
}

impl Cipher for ChaChaPoly {
    fn name(&self) -> &'static str {
        "ChaChaPoly"
    }

    fn set(&mut self, key: &[u8; 32]) {
        let mut key = chacha20poly1305::Key::from(*key);
        self.cipher = Some(ChaCha20Poly1305::new(&key));
        key.as_mut_slice().zeroize();
    }

    fn encrypt(&self, nonce: u64, authtext: &[u8], plaintext: &[u8], out: &mut [u8]) -> usize {
        let len = plaintext.len();
        // snow checks that `out` has room for the text and the tag before
        // it calls this, and sets a key first. Should either ever fail, the
        // answer is zero bytes, never the text in the clear: the message
        // then cannot be read, and the link fails.
        let (Some(cipher), Some(target)) = (self.cipher.as_ref(), out.get_mut(..len + TAG_BYTES)) else {
            return 0;
        };
        let (body, tag_out) = target.split_at_mut(len);
        body.copy_from_slice(plaintext);
        match cipher.encrypt_inout_detached(&nonce_bytes(nonce), authtext, body.into()) {
            Ok(tag) => {
                tag_out.copy_from_slice(&tag);
                len + TAG_BYTES
            }
            Err(_) => {
                body.zeroize();
                0
            }
        }
    }

    fn decrypt(&self, nonce: u64, authtext: &[u8], ciphertext: &[u8], out: &mut [u8]) -> Result<usize, snow::Error> {
        let len = ciphertext.len().checked_sub(TAG_BYTES).ok_or(snow::Error::Decrypt)?;
        let cipher = self.cipher.as_ref().ok_or(snow::Error::Decrypt)?;
        let body = out.get_mut(..len).ok_or(snow::Error::Decrypt)?;
        body.copy_from_slice(&ciphertext[..len]);
        let tag = chacha20poly1305::Tag::try_from(&ciphertext[len..]).map_err(|_| snow::Error::Decrypt)?;
        match cipher.decrypt_inout_detached(&nonce_bytes(nonce), authtext, (&mut *body).into(), &tag) {
            Ok(()) => Ok(len),
            Err(_) => {
                body.zeroize();
                Err(snow::Error::Decrypt)
            }
        }
    }
}

/// BLAKE2s with a 32 byte output. Its state is wiped when dropped (the hash
/// crate's `zeroize` feature).
#[derive(Default)]
struct Blake2s {
    hasher: Blake2s256,
}

impl Hash for Blake2s {
    fn name(&self) -> &'static str {
        "BLAKE2s"
    }

    fn block_len(&self) -> usize {
        64
    }

    fn hash_len(&self) -> usize {
        32
    }

    fn reset(&mut self) {
        self.hasher = Blake2s256::new();
    }

    fn input(&mut self, data: &[u8]) {
        Digest::update(&mut self.hasher, data);
    }

    fn result(&mut self, out: &mut [u8]) {
        let mut digest = std::mem::replace(&mut self.hasher, Blake2s256::new()).finalize();
        if let Some(target) = out.get_mut(..32) {
            target.copy_from_slice(&digest);
        }
        digest.as_mut_slice().zeroize();
    }
}

/// The resolver: the four primitives above, and nothing else.
struct Resolver;

impl CryptoResolver for Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        Some(Box::new(OsRandom))
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        match choice {
            DHChoice::Curve25519 => Some(Box::new(X25519::default())),
            _ => None,
        }
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<dyn Hash>> {
        match choice {
            HashChoice::Blake2s => Some(Box::new(Blake2s::default())),
            _ => None,
        }
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        match choice {
            CipherChoice::ChaChaPoly => Some(Box::new(ChaChaPoly::default())),
            _ => None,
        }
    }
}

fn internal(what: &'static str) -> impl Fn(snow::Error) -> LinkError {
    move |_| LinkError::Internal { what }
}

fn builder<'a>(protocol: &'static str) -> Result<Builder<'a>, LinkError> {
    let params: NoiseParams = protocol.parse().map_err(internal("read the name of a Noise protocol"))?;
    Ok(Builder::with_resolver(params, Box::new(Resolver)))
}

/// The phone's side of the pairing handshake.
pub(crate) fn pairing_phone(phone: &KeyPair, secret: &[u8; 32], prologue: &[u8]) -> Result<HandshakeState, LinkError> {
    builder(PAIRING_PROTOCOL)?
        .local_private_key(phone.expose_private())
        .and_then(|b| b.psk(3, secret))
        .and_then(|b| b.prologue(prologue))
        .and_then(|b| b.build_initiator())
        .map_err(internal("start a pairing handshake"))
}

/// The master's side of the pairing handshake.
pub(crate) fn pairing_master(
    master: &KeyPair,
    secret: &[u8; 32],
    prologue: &[u8],
) -> Result<HandshakeState, LinkError> {
    builder(PAIRING_PROTOCOL)?
        .local_private_key(master.expose_private())
        .and_then(|b| b.psk(3, secret))
        .and_then(|b| b.prologue(prologue))
        .and_then(|b| b.build_responder())
        .map_err(internal("start a pairing handshake"))
}

/// The phone's side of a session handshake.
pub(crate) fn session_phone(phone: &KeyPair, master: &PublicKey, prologue: &[u8]) -> Result<HandshakeState, LinkError> {
    builder(SESSION_PROTOCOL)?
        .local_private_key(phone.expose_private())
        .and_then(|b| b.remote_public_key(master.as_bytes()))
        .and_then(|b| b.prologue(prologue))
        .and_then(|b| b.build_initiator())
        .map_err(internal("start a session handshake"))
}

/// The master's side of a session handshake, for one phone it knows.
pub(crate) fn session_master(master: &KeyPair, phone: &PublicKey, prologue: &[u8]) -> Result<HandshakeState, LinkError> {
    builder(SESSION_PROTOCOL)?
        .local_private_key(master.expose_private())
        .and_then(|b| b.remote_public_key(phone.as_bytes()))
        .and_then(|b| b.prologue(prologue))
        .and_then(|b| b.build_responder())
        .map_err(internal("start a session handshake"))
}

/// A short hash for names and checks that are not secret: BLAKE2s of a
/// label, a zero byte and the data.
pub(crate) fn short_hash(label: &str, parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    Digest::update(&mut hasher, label.as_bytes());
    Digest::update(&mut hasher, [0u8]);
    for part in parts {
        Digest::update(&mut hasher, part);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use snow::TransportState;
    use snow::resolvers::{DefaultResolver, FallbackResolver};

    /// Fixed keys, so that two runs make the same bytes.
    struct Keys {
        phone: [u8; 32],
        master: [u8; 32],
        phone_e: [u8; 32],
        master_e: [u8; 32],
        psk: [u8; 32],
    }

    fn keys() -> Keys {
        Keys { phone: [1; 32], master: [2; 32], phone_e: [3; 32], master_e: [4; 32], psk: [5; 32] }
    }

    fn with<'a>(ours: bool, protocol: &'static str) -> Builder<'a> {
        let params: NoiseParams = protocol.parse().unwrap();
        if ours {
            Builder::with_resolver(params, Box::new(Resolver))
        } else {
            // snow's own resolver, with only its random numbers from here:
            // the tests fix every ephemeral key, so none are drawn.
            Builder::with_resolver(params, Box::new(FallbackResolver::new(Box::new(DefaultResolver), Box::new(Resolver))))
        }
    }

    fn public_of(private: &[u8; 32]) -> [u8; 32] {
        *KeyPair::from_private(private).public().as_bytes()
    }

    /// Runs a whole handshake and three messages each way, with the phone's
    /// side on one resolver and the master's on another, and returns every
    /// byte that crossed.
    fn run(protocol: &'static str, phone_ours: bool, master_ours: bool) -> Vec<Vec<u8>> {
        let k = keys();
        let master_public = public_of(&k.master);
        let phone_public = public_of(&k.phone);
        let prologue = b"LiMo test";
        let mut phone = with(phone_ours, protocol)
            .local_private_key(&k.phone)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&k.phone_e)
            .prologue(prologue)
            .unwrap();
        let mut master = with(master_ours, protocol)
            .local_private_key(&k.master)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&k.master_e)
            .prologue(prologue)
            .unwrap();
        if protocol == PAIRING_PROTOCOL {
            phone = phone.psk(3, &k.psk).unwrap();
            master = master.psk(3, &k.psk).unwrap();
        } else {
            phone = phone.remote_public_key(&master_public).unwrap();
            master = master.remote_public_key(&phone_public).unwrap();
        }
        let mut phone = phone.build_initiator().unwrap();
        let mut master = master.build_responder().unwrap();
        let mut crossed = Vec::new();
        let mut buf = [0u8; 1024];
        let mut payload = [0u8; 1024];
        let mut phone_turn = true;
        while !(phone.is_handshake_finished() && master.is_handshake_finished()) {
            let (from, to) = if phone_turn { (&mut phone, &mut master) } else { (&mut master, &mut phone) };
            let n = from.write_message(&[], &mut buf).unwrap();
            assert_eq!(to.read_message(&buf[..n], &mut payload).unwrap(), 0);
            crossed.push(buf[..n].to_vec());
            phone_turn = !phone_turn;
        }
        assert_eq!(phone.get_remote_static().unwrap(), &master_public);
        assert_eq!(master.get_remote_static().unwrap(), &phone_public);
        let mut phone: TransportState = phone.into_transport_mode().unwrap();
        let mut master: TransportState = master.into_transport_mode().unwrap();
        for i in 0..3u8 {
            let text = vec![i; 100 + usize::from(i)];
            let n = phone.write_message(&text, &mut buf).unwrap();
            let m = master.read_message(&buf[..n], &mut payload).unwrap();
            assert_eq!(&payload[..m], &text[..]);
            crossed.push(buf[..n].to_vec());
            let n = master.write_message(&text, &mut buf).unwrap();
            let m = phone.read_message(&buf[..n], &mut payload).unwrap();
            assert_eq!(&payload[..m], &text[..]);
            crossed.push(buf[..n].to_vec());
        }
        // Rekeying, which snow does through the cipher, must agree too.
        phone.rekey_outgoing();
        master.rekey_incoming();
        let n = phone.write_message(b"after a rekey", &mut buf).unwrap();
        let m = master.read_message(&buf[..n], &mut payload).unwrap();
        assert_eq!(&payload[..m], b"after a rekey");
        crossed.push(buf[..n].to_vec());
        crossed
    }

    #[test]
    fn the_resolver_matches_snows_own_byte_for_byte() {
        for protocol in [PAIRING_PROTOCOL, SESSION_PROTOCOL] {
            let snows = run(protocol, false, false);
            assert_eq!(run(protocol, true, true), snows, "{protocol}");
            assert_eq!(run(protocol, true, false), snows, "{protocol}");
            assert_eq!(run(protocol, false, true), snows, "{protocol}");
        }
    }

    #[test]
    fn the_handshake_messages_have_the_sizes_the_link_expects() {
        let pairing: Vec<usize> = run(PAIRING_PROTOCOL, true, true).iter().take(3).map(Vec::len).collect();
        assert_eq!(pairing, vec![PAIR_HELLO_BYTES, PAIR_REPLY_BYTES, PAIR_FINISH_BYTES]);
        assert_eq!(pairing, vec![48, 96, 64]);
        let session: Vec<usize> = run(SESSION_PROTOCOL, true, true).iter().take(2).map(Vec::len).collect();
        assert_eq!(session, vec![SESSION_HELLO_BYTES, SESSION_REPLY_BYTES]);
        assert_eq!(session, vec![48, 48]);
    }

    #[test]
    fn x25519_agrees_with_rfc_7748() {
        // RFC 7748, section 6.1.
        let alice = crate::encoding::unhex::<32>("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a").unwrap();
        let bob_public =
            crate::encoding::unhex::<32>("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f").unwrap();
        let mut dh = X25519::default();
        dh.set(&alice[..]);
        let mut out = [0u8; 32];
        dh.dh(&bob_public[..], &mut out).unwrap();
        assert_eq!(&*crate::encoding::hex(&out), "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
        // A public key that is too short is an error, not a panic.
        assert!(dh.dh(&[1, 2, 3], &mut out).is_err());
        assert!(dh.dh(&bob_public[..], &mut [0u8; 8]).is_err());
    }

    #[test]
    fn the_cipher_refuses_short_and_altered_text_without_panicking() {
        let mut c = ChaChaPoly::default();
        let mut out = [0u8; 64];
        // No key yet: nothing is encrypted and nothing decrypts.
        assert_eq!(c.encrypt(0, &[], b"hello", &mut out), 0);
        assert!(c.decrypt(0, &[], &[0u8; 20], &mut out).is_err());
        c.set(&[7; 32]);
        let n = c.encrypt(0, b"ad", b"hello", &mut out);
        assert_eq!(n, 5 + TAG_BYTES);
        let sealed = out[..n].to_vec();
        let mut back = [0u8; 64];
        assert_eq!(c.decrypt(0, b"ad", &sealed, &mut back).unwrap(), 5);
        assert_eq!(&back[..5], b"hello");
        assert!(c.decrypt(1, b"ad", &sealed, &mut back).is_err());
        assert!(c.decrypt(0, b"xx", &sealed, &mut back).is_err());
        let mut altered = sealed.clone();
        altered[0] ^= 1;
        assert!(c.decrypt(0, b"ad", &altered, &mut back).is_err());
        assert!(c.decrypt(0, b"ad", &sealed[..10], &mut back).is_err());
        assert!(c.decrypt(0, b"ad", &sealed, &mut [0u8; 2]).is_err());
        // No room for the tag: zero bytes, never the text.
        let mut small = [0u8; 8];
        assert_eq!(c.encrypt(0, &[], b"hello", &mut small), 0);
        assert_eq!(small, [0u8; 8]);
    }

    #[test]
    fn blake2s_known_answer() {
        // BLAKE2s-256 of "abc", from RFC 7693, appendix B.
        let mut h = Blake2s::default();
        h.input(b"abc");
        let mut out = [0u8; 32];
        h.result(&mut out);
        assert_eq!(&*crate::encoding::hex(&out), "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982");
        // And the state starts again after a result.
        h.input(b"abc");
        let mut again = [0u8; 32];
        h.result(&mut again);
        assert_eq!(again, out);
    }
}
