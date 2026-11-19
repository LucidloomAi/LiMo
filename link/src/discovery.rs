//! How the master announces itself on the home network, as names and
//! values only. The callers do the announcing and the searching: the master
//! with `mdns-sd`, the phone with Android's `NsdManager`, as proof four
//! showed on real hardware.
//!
//! | Item | Value |
//! | --- | --- |
//! | Service type | `_limo._tcp.local.` (`_limo._tcp` for `NsdManager`) |
//! | Host name | `limo-<first 8 hex of the master id>.local.` |
//! | Instance name | `LiMo <first 8 hex of the master id>` |
//! | Port | 47810 unless changed |
//! | TXT | `v=1`, `id=<the master id, 16 hex>` |
//!
//! The master id is made from the master's public key, so a phone that
//! holds the key (from the offer) knows which announcement is its master's
//! before it connects. The PC's own name is never announced.

use std::fmt;

use crate::crypto::short_hash;
use crate::encoding::{hex, unhex};
use crate::keys::PublicKey;

/// The DNS-SD service type, as `mdns-sd` wants it.
pub const SERVICE_TYPE: &str = "_limo._tcp.local.";

/// The DNS-SD service type, as Android's `NsdManager` wants it.
pub const NSD_SERVICE_TYPE: &str = "_limo._tcp";

/// The TCP port the master listens on unless the guardian changes it.
pub const DEFAULT_PORT: u16 = 47810;

/// The length of a master id.
pub const MASTER_ID_BYTES: usize = 8;

/// A master's id: the first 8 bytes of BLAKE2s over the label
/// `LiMo master id v1`, a zero byte and the master's public key. Not
/// secret: it is announced on the network.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MasterId([u8; MASTER_ID_BYTES]);

impl MasterId {
    /// The id of the master with this public key.
    pub fn of(key: &PublicKey) -> MasterId {
        let hash = short_hash("LiMo master id v1", &[key.as_bytes()]);
        let mut id = [0u8; MASTER_ID_BYTES];
        id.copy_from_slice(&hash[..MASTER_ID_BYTES]);
        MasterId(id)
    }

    /// Its 16 lowercase hex characters, as in the TXT record.
    pub fn to_hex(&self) -> String {
        hex(&self.0).to_string()
    }

    /// Its first 8 hex characters, as in the host and instance names.
    pub fn short(&self) -> String {
        hex(&self.0[..4]).to_string()
    }

    /// The id from its 16 hex characters, in either case.
    pub fn from_hex(text: &str) -> Option<MasterId> {
        unhex::<MASTER_ID_BYTES>(text).map(|bytes| MasterId(*bytes))
    }
}

impl fmt::Debug for MasterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MasterId({})", self.to_hex())
    }
}

impl fmt::Display for MasterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The host name the master announces: `limo-<8 hex>.local.`
pub fn host_name(id: &MasterId) -> String {
    format!("limo-{}.local.", id.short())
}

/// The instance name the master announces: `LiMo <8 hex>`.
pub fn instance_name(id: &MasterId) -> String {
    format!("LiMo {}", id.short())
}

/// The TXT records the master announces, as key and value: `v=1` and
/// `id=<16 hex>`.
pub fn txt_records(id: &MasterId) -> Vec<(String, String)> {
    vec![("v".to_string(), crate::LINK_VERSION.to_string()), ("id".to_string(), id.to_hex())]
}

/// What an announcement's TXT records say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Announced {
    /// The link version the master speaks.
    pub version: u8,
    /// The master's id.
    pub id: MasterId,
}

/// Why an announcement's TXT records are not a LiMo master's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxtProblem {
    /// No `v` record, or one that is not a number from 1 to 255.
    NoVersion,
    /// No `id` record, or one that is not 16 hex characters.
    NoId,
}

impl fmt::Display for TxtProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxtProblem::NoVersion => write!(f, "the announcement says no link version"),
            TxtProblem::NoId => write!(f, "the announcement has no master id"),
        }
    }
}

impl std::error::Error for TxtProblem {}

/// Reads the TXT records of an announcement found on the network, as key
/// and value pairs (`NsdServiceInfo.getAttributes` on Android, with the
/// values as text). Keys are matched without regard to case, as DNS-SD
/// asks; records it does not know are ignored, so a later master may add
/// some.
pub fn read_txt<K: AsRef<str>, V: AsRef<str>>(records: &[(K, V)]) -> Result<Announced, TxtProblem> {
    let find = |key: &str| {
        records.iter().find(|(k, _)| k.as_ref().eq_ignore_ascii_case(key)).map(|(_, v)| v.as_ref())
    };
    let version = find("v").and_then(|v| v.parse::<u8>().ok()).filter(|&v| v > 0).ok_or(TxtProblem::NoVersion)?;
    let id = find("id").and_then(MasterId::from_hex).ok_or(TxtProblem::NoId)?;
    Ok(Announced { version, id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_made_from_the_key_and_never_from_the_pc() {
        let key = PublicKey::from_bytes([7; 32]);
        let id = MasterId::of(&key);
        assert_eq!(id, MasterId::of(&key));
        assert_ne!(id, MasterId::of(&PublicKey::from_bytes([8; 32])));
        assert_eq!(id.to_hex().len(), 16);
        assert_eq!(id.short(), id.to_hex()[..8]);
        assert_eq!(host_name(&id), format!("limo-{}.local.", &id.to_hex()[..8]));
        assert_eq!(instance_name(&id), format!("LiMo {}", &id.to_hex()[..8]));
        assert_eq!(MasterId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(MasterId::from_hex(&id.to_hex().to_uppercase()), Some(id));
        // A known answer, worked out separately with Python's hashlib, so
        // that the id never changes by accident: the PC and the phones must
        // agree on it for years.
        assert_eq!(id.to_hex(), "25f61b7533af1b79");
    }

    #[test]
    fn txt_records_both_ways() {
        let id = MasterId::of(&PublicKey::from_bytes([1; 32]));
        let records = txt_records(&id);
        assert_eq!(records, vec![("v".into(), "1".into()), ("id".into(), id.to_hex())]);
        assert_eq!(read_txt(&records), Ok(Announced { version: 1, id }));
        let upper = id.to_hex().to_uppercase();
        let shouted = [("V", "1"), ("ID", upper.as_str()), ("extra", "x")];
        assert_eq!(read_txt(&shouted), Ok(Announced { version: 1, id }));
        assert_eq!(read_txt::<&str, &str>(&[]), Err(TxtProblem::NoVersion));
        assert_eq!(read_txt(&[("v", "0"), ("id", "0011223344556677")]), Err(TxtProblem::NoVersion));
        assert_eq!(read_txt(&[("v", "x"), ("id", "0011223344556677")]), Err(TxtProblem::NoVersion));
        assert_eq!(read_txt(&[("v", "2")]), Err(TxtProblem::NoId));
        assert_eq!(read_txt(&[("v", "2"), ("id", "00112233")]), Err(TxtProblem::NoId));
        assert_eq!(
            read_txt(&[("v", "2"), ("id", "0011223344556677")]),
            Ok(Announced { version: 2, id: MasterId::from_hex("0011223344556677").unwrap() })
        );
    }
}
