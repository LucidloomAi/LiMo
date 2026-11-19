//! The pairing offer: what the master PC hands a phone so that the phone
//! can pair with it.
//!
//! The master makes an offer when a guardian adds a phone. The child's
//! phone gets it over the USB cable while the PC installs LiMo on it; a
//! guardian's phone would scan it as a QR code. It holds the master's
//! public key, a random id, a random 32 byte secret, when it expires (10
//! minutes after it was made, by the master's clock), the port and the
//! master's IPv4 addresses. It works once.
//!
//! Its text form is `limo1:` and the base32 of its bytes:
//!
//! | Bytes | What |
//! | --- | --- |
//! | 32 | the master's public key |
//! | 8 | the offer's id |
//! | 32 | the secret |
//! | 8 | when it expires, unix seconds, big endian |
//! | 2 | the port, big endian |
//! | 1 | how many addresses follow, 0 to 8 |
//! | 4 each | the IPv4 addresses |
//!
//! The secret is the pairing handshake's pre-shared key. Whoever holds the
//! offer can pair one phone with the master until it expires or is used, so
//! the offer is shown only as long as it is needed, and the secret never
//! shows in `Debug` output or in an error, and is wiped when dropped.

use std::fmt;
use std::net::Ipv4Addr;

use zeroize::Zeroizing;

use crate::discovery::MasterId;
use crate::encoding::{Base32Problem, base32, hex, unbase32};
use crate::error::LinkError;
use crate::keys::{KEY_BYTES, PublicKey};

/// What the text form of an offer starts with.
pub const OFFER_PREFIX: &str = "limo1:";

/// How long an offer lasts: 10 minutes.
pub const OFFER_LIFETIME_SECS: i64 = 600;

/// The most addresses an offer carries.
pub const MAX_OFFER_ADDRESSES: usize = 8;

/// The length of an offer's id.
pub const OFFER_ID_BYTES: usize = 8;

/// The length of an offer's secret.
pub const OFFER_SECRET_BYTES: usize = 32;

/// The bytes of an offer before its addresses.
const FIXED_BYTES: usize = KEY_BYTES + OFFER_ID_BYTES + OFFER_SECRET_BYTES + 8 + 2 + 1;

/// An offer's id: 8 random bytes. Not secret: it goes in the clear at the
/// start of the pairing, so the master knows which offer is meant.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OfferId([u8; OFFER_ID_BYTES]);

impl OfferId {
    /// The id from its 8 bytes.
    pub fn from_bytes(bytes: [u8; OFFER_ID_BYTES]) -> OfferId {
        OfferId(bytes)
    }

    /// Its 8 bytes.
    pub fn as_bytes(&self) -> &[u8; OFFER_ID_BYTES] {
        &self.0
    }

    /// Its 16 lowercase hex characters.
    pub fn to_hex(&self) -> String {
        hex(&self.0).to_string()
    }
}

impl fmt::Debug for OfferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OfferId({})", self.to_hex())
    }
}

/// Why an offer's text cannot be used. Never quotes the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferProblem {
    /// It does not start with `limo1:`, or any other `limo<n>:`.
    NotAnOffer,
    /// It was made by a newer LiMo: `limo2:` or later.
    NewerVersion,
    /// A character that base32 does not have, at this place in the text
    /// (counted from 1).
    BadCharacter {
        /// Where.
        at: usize,
    },
    /// Its length is not that of an offer: it was cut short or run on.
    WrongLength,
    /// It has more than 8 addresses.
    TooManyAddresses,
    /// Its port is 0.
    NoPort,
}

impl fmt::Display for OfferProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OfferProblem::NotAnOffer => write!(f, "it is not a LiMo pairing offer"),
            OfferProblem::NewerVersion => write!(f, "it was made by a newer LiMo; update LiMo on this phone"),
            OfferProblem::BadCharacter { at } => write!(f, "character {at} of it cannot be in an offer"),
            OfferProblem::WrongLength => write!(f, "it is not whole: part of it is missing, or something was added"),
            OfferProblem::TooManyAddresses => write!(f, "it lists more than {MAX_OFFER_ADDRESSES} addresses"),
            OfferProblem::NoPort => write!(f, "its port is 0"),
        }
    }
}

impl std::error::Error for OfferProblem {}

/// A pairing offer.
#[derive(Clone, PartialEq, Eq)]
pub struct Offer {
    master_key: PublicKey,
    id: OfferId,
    secret: Zeroizing<[u8; OFFER_SECRET_BYTES]>,
    expires_at: i64,
    port: u16,
    addresses: Vec<Ipv4Addr>,
}

impl Offer {
    /// A new offer from this master, with a random id and secret, lasting
    /// [`OFFER_LIFETIME_SECS`] from `now` (unix seconds, by the master's
    /// clock). The addresses are the master's own IPv4 addresses on the
    /// home network, at most 8, best first.
    pub fn new(master_key: PublicKey, port: u16, addresses: Vec<Ipv4Addr>, now: i64) -> Result<Offer, LinkError> {
        if port == 0 {
            return Err(OfferProblem::NoPort.into());
        }
        if addresses.len() > MAX_OFFER_ADDRESSES {
            return Err(OfferProblem::TooManyAddresses.into());
        }
        let mut id = [0u8; OFFER_ID_BYTES];
        getrandom::fill(&mut id).map_err(|_| LinkError::NoRandomness)?;
        let mut secret = Zeroizing::new([0u8; OFFER_SECRET_BYTES]);
        getrandom::fill(&mut secret[..]).map_err(|_| LinkError::NoRandomness)?;
        Ok(Offer {
            master_key,
            id: OfferId(id),
            secret,
            expires_at: now.saturating_add(OFFER_LIFETIME_SECS),
            port,
            addresses,
        })
    }

    /// The master's public key: the phone pins it, and it is the only key
    /// the phone will accept from the master, ever.
    pub fn master_key(&self) -> PublicKey {
        self.master_key
    }

    /// The master's id, made from its key, as it announces itself on the
    /// network.
    pub fn master_id(&self) -> MasterId {
        MasterId::of(&self.master_key)
    }

    /// The offer's id.
    pub fn id(&self) -> OfferId {
        self.id
    }

    /// When it expires, unix seconds by the master's clock.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// True once it has expired, at `now` by the master's clock. The master
    /// decides; a phone's own clock may be wrong, so a phone uses this only
    /// to say that an offer looks old.
    pub fn is_expired(&self, now: i64) -> bool {
        now > self.expires_at
    }

    /// The port the master listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The master's IPv4 addresses, best first.
    pub fn addresses(&self) -> &[Ipv4Addr] {
        &self.addresses
    }

    /// The secret, for the pairing handshake.
    pub(crate) fn secret(&self) -> &[u8; OFFER_SECRET_BYTES] {
        &self.secret
    }

    /// The text form, `limo1:` and base32, in a string that is wiped when
    /// dropped. It holds the secret: show it only as long as it is needed.
    pub fn to_text(&self) -> Zeroizing<String> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(FIXED_BYTES + 4 * MAX_OFFER_ADDRESSES));
        bytes.extend_from_slice(self.master_key.as_bytes());
        bytes.extend_from_slice(&self.id.0);
        bytes.extend_from_slice(&self.secret[..]);
        bytes.extend_from_slice(&self.expires_at.to_be_bytes());
        bytes.extend_from_slice(&self.port.to_be_bytes());
        // At most 8, as `new` and `from_text` make sure.
        bytes.push(u8::try_from(self.addresses.len()).unwrap_or(u8::MAX));
        for address in &self.addresses {
            bytes.extend_from_slice(&address.octets());
        }
        let coded = base32(&bytes);
        let mut text = Zeroizing::new(String::with_capacity(OFFER_PREFIX.len() + coded.len()));
        text.push_str(OFFER_PREFIX);
        text.push_str(&coded);
        text
    }

    /// Reads the text form. Spaces and line ends around it are ignored, and
    /// the base32 may be in either case.
    pub fn from_text(text: &str) -> Result<Offer, OfferProblem> {
        let text = text.trim();
        let Some(colon) = text.find(':') else {
            return Err(OfferProblem::NotAnOffer);
        };
        let (prefix, coded) = (&text[..colon + 1], &text[colon + 1..]);
        if !prefix.eq_ignore_ascii_case(OFFER_PREFIX) {
            let version = prefix.get(4..prefix.len() - 1).filter(|_| prefix.len() > 5);
            let is_limo = prefix.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("limo"));
            return match version.and_then(|v| v.parse::<u32>().ok()) {
                Some(v) if is_limo && v > 1 => Err(OfferProblem::NewerVersion),
                _ => Err(OfferProblem::NotAnOffer),
            };
        }
        let bytes = unbase32(coded).map_err(|problem| match problem {
            Base32Problem::BadCharacter(at) => OfferProblem::BadCharacter { at: at + OFFER_PREFIX.len() },
            Base32Problem::NotCanonical => OfferProblem::WrongLength,
        })?;
        if bytes.len() < FIXED_BYTES {
            return Err(OfferProblem::WrongLength);
        }
        let count = usize::from(bytes[FIXED_BYTES - 1]);
        if count > MAX_OFFER_ADDRESSES {
            return Err(OfferProblem::TooManyAddresses);
        }
        if bytes.len() != FIXED_BYTES + 4 * count {
            return Err(OfferProblem::WrongLength);
        }
        let mut at = 0;
        let mut take = |n: usize| {
            let part = &bytes[at..at + n];
            at += n;
            part
        };
        let mut key = [0u8; KEY_BYTES];
        key.copy_from_slice(take(KEY_BYTES));
        let mut id = [0u8; OFFER_ID_BYTES];
        id.copy_from_slice(take(OFFER_ID_BYTES));
        let mut secret = Zeroizing::new([0u8; OFFER_SECRET_BYTES]);
        secret.copy_from_slice(take(OFFER_SECRET_BYTES));
        let mut expires = [0u8; 8];
        expires.copy_from_slice(take(8));
        let mut port = [0u8; 2];
        port.copy_from_slice(take(2));
        take(1);
        let port = u16::from_be_bytes(port);
        if port == 0 {
            return Err(OfferProblem::NoPort);
        }
        let addresses = (0..count)
            .map(|_| {
                let mut octets = [0u8; 4];
                octets.copy_from_slice(take(4));
                Ipv4Addr::from(octets)
            })
            .collect();
        Ok(Offer {
            master_key: PublicKey::from_bytes(key),
            id: OfferId(id),
            secret,
            expires_at: i64::from_be_bytes(expires),
            port,
            addresses,
        })
    }
}

impl fmt::Debug for Offer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Offer")
            .field("master_key", &self.master_key)
            .field("id", &self.id)
            .field("secret", &format_args!("hidden"))
            .field("expires_at", &self.expires_at)
            .field("port", &self.port)
            .field("addresses", &self.addresses)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyPair;

    fn sample() -> Offer {
        let key = KeyPair::generate().unwrap().public();
        Offer::new(key, 47810, vec![Ipv4Addr::new(192, 168, 1, 20), Ipv4Addr::new(10, 0, 0, 5)], 1_800_000_000).unwrap()
    }

    #[test]
    fn an_offer_goes_to_text_and_back_unchanged() {
        let offer = sample();
        let text = offer.to_text();
        assert!(text.starts_with("limo1:"));
        // 83 bytes and two addresses: 91 bytes, 146 characters of base32.
        assert_eq!(text.len(), 6 + 146);
        assert!(text[6..].bytes().all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b)));
        assert_eq!(Offer::from_text(&text).unwrap(), offer);
        assert_eq!(Offer::from_text(&text.to_lowercase()).unwrap(), offer);
        assert_eq!(Offer::from_text(&format!("  {}\r\n", text.as_str())).unwrap(), offer);
        assert_eq!(Offer::from_text(&format!("LIMO1:{}", &text[6..])).unwrap(), offer);
        assert_eq!(offer.expires_at(), 1_800_000_600);
        assert!(!offer.is_expired(1_800_000_600));
        assert!(offer.is_expired(1_800_000_601));
    }

    #[test]
    fn offers_with_no_address_and_with_eight_go_through() {
        let key = KeyPair::generate().unwrap().public();
        for n in [0usize, 1, 8] {
            let addresses = (0..n).map(|i| Ipv4Addr::new(192, 168, 0, i as u8)).collect();
            let offer = Offer::new(key, 1, addresses, 0).unwrap();
            assert_eq!(Offer::from_text(&offer.to_text()).unwrap(), offer);
        }
        let nine = (0..9).map(|i| Ipv4Addr::new(10, 0, 0, i)).collect();
        assert_eq!(Offer::new(key, 1, nine, 0), Err(LinkError::BadOffer(OfferProblem::TooManyAddresses)));
        assert_eq!(Offer::new(key, 0, vec![], 0), Err(LinkError::BadOffer(OfferProblem::NoPort)));
    }

    #[test]
    fn two_offers_never_share_an_id_or_a_secret() {
        let a = sample();
        let b = sample();
        assert_ne!(a.id(), b.id());
        assert_ne!(a.secret(), b.secret());
    }

    #[test]
    fn broken_offers_are_refused_without_quoting_them() {
        let offer = sample();
        let text = offer.to_text();
        let coded = &text[6..];
        let cases: Vec<(String, OfferProblem)> = vec![
            (String::new(), OfferProblem::NotAnOffer),
            ("hello".into(), OfferProblem::NotAnOffer),
            (format!("limo:{coded}"), OfferProblem::NotAnOffer),
            (format!("lime1:{coded}"), OfferProblem::NotAnOffer),
            (format!("limo2:{coded}"), OfferProblem::NewerVersion),
            (format!("limo12:{coded}"), OfferProblem::NewerVersion),
            (format!("limo1:{}", &coded[..coded.len() - 8]), OfferProblem::WrongLength),
            (format!("limo1:{coded}AAAAAAAA"), OfferProblem::WrongLength),
            (format!("limo1:{}1{}", &coded[..10], &coded[11..]), OfferProblem::BadCharacter { at: 17 }),
            (format!("limo1:{} {}", &coded[..10], &coded[10..]), OfferProblem::BadCharacter { at: 17 }),
            ("limo1:".into(), OfferProblem::WrongLength),
        ];
        let secret_hex = hex(offer.secret());
        for (text, problem) in cases {
            assert_eq!(Offer::from_text(&text), Err(problem), "{problem:?}");
            assert!(!problem.to_string().contains(&secret_hex[..8]));
        }
    }

    #[test]
    fn an_offer_never_shows_its_secret() {
        let offer = sample();
        let shown = format!("{offer:?}");
        let secret_hex = hex(offer.secret());
        assert!(!shown.contains(secret_hex.as_str()), "{shown}");
        assert!(shown.contains("hidden"));
        assert!(shown.contains(&offer.master_key().to_hex()));
        // The bytes of the secret as Rust would print an array, too.
        let as_array = format!("{:?}", &offer.secret()[..4]);
        assert!(!shown.contains(&as_array[1..as_array.len() - 1]));
    }
}
