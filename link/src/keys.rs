//! Static keys: each phone's and the master's own X25519 key pair.
//!
//! A key pair is made once (the master's when LiMo is set up, a phone's
//! when it pairs) and kept by its owner in private storage. The public half
//! travels: the master's in the pairing offer, a phone's in the pairing
//! handshake. The private half never leaves its device, never shows in
//! `Debug` output or an error, and is wiped from memory when dropped.

use std::fmt;

use curve25519_dalek::montgomery::MontgomeryPoint;
use serde::de::{self, Deserializer};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::encoding::{hex, unhex};
use crate::error::LinkError;

/// The length of a key, public or private, in bytes.
pub const KEY_BYTES: usize = 32;

/// An X25519 public key. Public: it may be shown, logged and sent.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicKey([u8; KEY_BYTES]);

impl PublicKey {
    /// The key from its 32 bytes.
    pub fn from_bytes(bytes: [u8; KEY_BYTES]) -> PublicKey {
        PublicKey(bytes)
    }

    /// Its 32 bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }

    /// Its 64 lowercase hex characters.
    pub fn to_hex(&self) -> String {
        hex(&self.0).to_string()
    }

    /// The key from 64 hex characters, in either case.
    pub fn from_hex(text: &str) -> Option<PublicKey> {
        unhex::<KEY_BYTES>(text).map(|bytes| PublicKey(*bytes))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.to_hex())
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PublicKey, D::Error> {
        deserializer.deserialize_str(HexVisitor::<KEY_BYTES>).map(|bytes| PublicKey(*bytes))
    }
}

/// Reads exactly `N` bytes from a hex string, straight into memory that is
/// wiped when dropped. The error never quotes the string.
pub(crate) struct HexVisitor<const N: usize>;

impl<const N: usize> de::Visitor<'_> for HexVisitor<N> {
    type Value = Zeroizing<[u8; N]>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} hex characters", N * 2)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        unhex::<N>(text).ok_or_else(|| E::custom(format_args!("not {} hex characters", N * 2)))
    }
}

/// An X25519 key pair: a static key of a phone or of the master.
///
/// Its `Debug` output shows the public key only, and the private key is
/// wiped from memory when the pair is dropped. Serialized (to be kept in
/// private storage), it is `{"public": hex, "private": hex}`; serialize it
/// only into a buffer that is wiped, as [`crate::PairedMaster::to_bytes`]
/// does.
#[derive(Clone)]
pub struct KeyPair {
    private: Zeroizing<[u8; KEY_BYTES]>,
    public: PublicKey,
}

impl KeyPair {
    /// A new key pair from the operating system's random numbers.
    pub fn generate() -> Result<KeyPair, LinkError> {
        let mut private = Zeroizing::new([0u8; KEY_BYTES]);
        getrandom::fill(&mut private[..]).map_err(|_| LinkError::NoRandomness)?;
        Ok(KeyPair::from_private(&private))
    }

    /// The key pair whose private key is these 32 bytes, as kept. The
    /// caller's copy is the caller's to wipe.
    pub fn from_private(private: &[u8; KEY_BYTES]) -> KeyPair {
        let private = Zeroizing::new(*private);
        let mut point = MontgomeryPoint::mul_base_clamped(*private);
        let public = PublicKey(point.to_bytes());
        point.zeroize();
        KeyPair { private, public }
    }

    /// The public half.
    pub fn public(&self) -> PublicKey {
        self.public
    }

    /// The private half, to keep in private storage and for the handshakes.
    /// Never show it or log it.
    pub fn expose_private(&self) -> &[u8; KEY_BYTES] {
        &self.private
    }
}

impl PartialEq for KeyPair {
    /// Two pairs are the same when their public keys are: the public key is
    /// made from the private one.
    fn eq(&self, other: &KeyPair) -> bool {
        self.public == other.public
    }
}

impl Eq for KeyPair {}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair").field("public", &self.public).field("private", &format_args!("hidden")).finish()
    }
}

impl Serialize for KeyPair {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("KeyPair", 2)?;
        out.serialize_field("public", &self.public)?;
        out.serialize_field("private", &*hex(&self.private[..]))?;
        out.end()
    }
}

impl<'de> Deserialize<'de> for KeyPair {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<KeyPair, D::Error> {
        deserializer.deserialize_struct("KeyPair", &["public", "private"], KeyPairVisitor)
    }
}

/// The private key as it is read from a saved key pair: wiped when dropped.
struct PrivateField(Zeroizing<[u8; KEY_BYTES]>);

impl<'de> Deserialize<'de> for PrivateField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PrivateField, D::Error> {
        deserializer.deserialize_str(HexVisitor::<KEY_BYTES>).map(PrivateField)
    }
}

struct KeyPairVisitor;

impl<'de> de::Visitor<'de> for KeyPairVisitor {
    type Value = KeyPair;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a key pair")
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<KeyPair, A::Error> {
        let mut public: Option<PublicKey> = None;
        let mut private: Option<PrivateField> = None;
        while let Some(name) = map.next_key::<FieldName>()? {
            match name {
                FieldName::Public if public.is_none() => public = Some(map.next_value()?),
                FieldName::Private if private.is_none() => private = Some(map.next_value()?),
                _ => return Err(de::Error::custom("a field twice")),
            }
        }
        let public = public.ok_or_else(|| de::Error::missing_field("public"))?;
        let private = private.ok_or_else(|| de::Error::missing_field("private"))?;
        let pair = KeyPair::from_private(&private.0);
        if pair.public != public {
            return Err(de::Error::custom("a public key that is not the private key's"));
        }
        Ok(pair)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum FieldName {
    Public,
    Private,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x25519_known_answer_of_rfc_7748() {
        // RFC 7748, section 6.1: Alice's private key and public key.
        let private = unhex::<32>("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a").unwrap();
        let pair = KeyPair::from_private(&private);
        assert_eq!(pair.public().to_hex(), "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        // And Bob's.
        let private = unhex::<32>("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb").unwrap();
        assert_eq!(
            KeyPair::from_private(&private).public().to_hex(),
            "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
        );
    }

    #[test]
    fn generated_pairs_differ_and_rebuild_from_their_private_half() {
        let a = KeyPair::generate().unwrap();
        let b = KeyPair::generate().unwrap();
        assert_ne!(a.public(), b.public());
        assert_eq!(KeyPair::from_private(a.expose_private()), a);
    }

    #[test]
    fn a_pair_saved_and_read_again_is_the_same_and_never_shows_its_private_half() {
        let pair = KeyPair::generate().unwrap();
        let private_hex = hex(pair.expose_private());
        let shown = format!("{pair:?}");
        assert!(!shown.contains(private_hex.as_str()), "{shown}");
        assert!(shown.contains(&pair.public().to_hex()));
        let json = serde_json::to_string(&pair).unwrap();
        let back: KeyPair = serde_json::from_str(&json).unwrap();
        assert_eq!(back, pair);
        assert_eq!(back.expose_private(), pair.expose_private());
    }

    #[test]
    fn a_saved_pair_that_does_not_fit_is_refused_without_quoting_it() {
        let pair = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();
        let private_hex = hex(pair.expose_private());
        let mismatched = format!(r#"{{"public":"{}","private":"{}"}}"#, other.public().to_hex(), private_hex.as_str());
        let short = format!(r#"{{"public":"{}","private":"{}"}}"#, pair.public().to_hex(), &private_hex[..62]);
        let twice = format!(
            r#"{{"public":"{}","private":"{}","private":"{}"}}"#,
            pair.public().to_hex(),
            private_hex.as_str(),
            private_hex.as_str()
        );
        let extra = format!(r#"{{"public":"{}","private":"{}","x":1}}"#, pair.public().to_hex(), private_hex.as_str());
        for json in [mismatched, short, twice, extra, r#"{"public":"00"}"#.to_string()] {
            let error = serde_json::from_str::<KeyPair>(&json).unwrap_err().to_string();
            assert!(!error.contains(&private_hex[..16]), "{error}");
        }
    }

    #[test]
    fn public_keys_in_hex_and_json() {
        let key = KeyPair::generate().unwrap().public();
        assert_eq!(PublicKey::from_hex(&key.to_hex()), Some(key));
        assert_eq!(PublicKey::from_hex(&key.to_hex().to_uppercase()), Some(key));
        assert_eq!(PublicKey::from_hex("00"), None);
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, format!("\"{}\"", key.to_hex()));
        assert_eq!(serde_json::from_str::<PublicKey>(&json).unwrap(), key);
        assert!(serde_json::from_str::<PublicKey>("\"zz\"").is_err());
        assert!(serde_json::from_str::<PublicKey>("[1,2]").is_err());
    }
}
