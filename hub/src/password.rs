//! The guardian password, kept only as a salted slow hash.
//!
//! PBKDF2-HMAC-SHA256 with [`PASSWORD_ROUNDS`] rounds, a 16 byte salt and a
//! 32 byte hash: the same two RustCrypto pieces the core uses for the PIN.
//! The rounds are stored with the record, so a later version can raise them;
//! a record with fewer rounds than today's is rehashed at the next right
//! password.

use std::fmt;

use ctutils::CtEq;
use sha2::Sha256;

use crate::error::HubError;
use crate::random;

/// PBKDF2 rounds for a new password record: 600,000, the 2023 OWASP figure
/// for PBKDF2-HMAC-SHA256. About a tenth of a second on a 2023 desktop; a
/// password is checked when a guardian signs in, not on every exchange.
pub const PASSWORD_ROUNDS: u32 = 600_000;
/// The shortest password, in characters.
pub const MIN_PASSWORD_CHARS: u32 = 8;
/// The longest password, in characters.
pub const MAX_PASSWORD_CHARS: u32 = 256;
/// The most rounds a stored record may ask for, so that a damaged file cannot
/// make one check take minutes.
pub(crate) const MAX_ROUNDS: u32 = 20_000_000;
/// The salt length, in bytes.
const SALT_BYTES: usize = 16;
/// The hash length, in bytes.
const HASH_BYTES: usize = 32;

/// A password record: salt, rounds and hash. Its `Debug` hides the salt and
/// the hash.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PasswordRecord {
    pub(crate) salt: Vec<u8>,
    pub(crate) rounds: u32,
    pub(crate) hash: Vec<u8>,
}

impl fmt::Debug for PasswordRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PasswordRecord {{ rounds: {}, salt: hidden ({} bytes), hash: hidden ({} bytes) }}",
            self.rounds,
            self.salt.len(),
            self.hash.len()
        )
    }
}

fn derive(password: &str, salt: &[u8], rounds: u32) -> Vec<u8> {
    let mut out = vec![0u8; HASH_BYTES];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, rounds, &mut out);
    out
}

/// Why a new password cannot be taken, if it cannot.
pub(crate) fn check_new(password: &str) -> Result<(), HubError> {
    let chars = password.chars().count() as u64;
    if chars < MIN_PASSWORD_CHARS as u64 {
        return Err(HubError::PasswordTooShort { min_chars: MIN_PASSWORD_CHARS });
    }
    if chars > MAX_PASSWORD_CHARS as u64 {
        return Err(HubError::PasswordTooLong { max_chars: MAX_PASSWORD_CHARS });
    }
    Ok(())
}

impl PasswordRecord {
    /// The record for a new password, with a new salt, at today's cost.
    /// Refuses a password that is too short or too long.
    pub(crate) fn new_password(password: &str) -> Result<PasswordRecord, HubError> {
        check_new(password)?;
        PasswordRecord::rehash(password)
    }

    /// The record for a password already known to be right, with a new salt,
    /// at today's cost. Slow on purpose.
    pub(crate) fn rehash(password: &str) -> Result<PasswordRecord, HubError> {
        let salt = random::bytes(SALT_BYTES)?;
        let hash = derive(password, &salt, PASSWORD_ROUNDS);
        Ok(PasswordRecord { salt, rounds: PASSWORD_ROUNDS, hash })
    }

    /// True when the record is one this crate could have made.
    pub(crate) fn is_sound(&self) -> bool {
        self.salt.len() >= SALT_BYTES && (1..=MAX_ROUNDS).contains(&self.rounds) && self.hash.len() == HASH_BYTES
    }

    /// True when `password` is the password of this record. The hashes are
    /// compared in constant time. A password longer than any that can be set
    /// is never right and is not hashed.
    pub(crate) fn matches(&self, password: &str) -> bool {
        if !self.is_sound() || password.chars().count() as u64 > MAX_PASSWORD_CHARS as u64 {
            return false;
        }
        derive(password, &self.salt, self.rounds)[..].ct_eq(self.hash.as_slice()).to_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Known answers from Python's `hashlib.pbkdf2_hmac("sha256", ...)`, so
    /// that the stored form never changes by accident.
    #[test]
    fn known_answers() {
        let salt: Vec<u8> = (0u8..16).collect();
        assert_eq!(
            hex(&derive("correct horse", &salt, 1)),
            "c5199f52d095f03bb27ace7b385711999c67ec4aacd0f298a6da36ce47520daf"
        );
        assert_eq!(
            hex(&derive("correct horse", &salt, 1_000)),
            "c914cc4f06cc6e8f46d157e3a1b5aa7abceebb17bb0444cd4c4ac16ca2ae9864"
        );
    }

    #[test]
    fn a_record_matches_its_password_only() {
        let record = PasswordRecord::new_password("correct horse").unwrap();
        assert_eq!(record.rounds, PASSWORD_ROUNDS);
        assert!(record.matches("correct horse"));
        assert!(!record.matches("correct horsE"));
        assert!(!record.matches(""));
        assert!(!record.matches(&"x".repeat(10_000)));
        let mut broken = record.clone();
        broken.rounds = 0;
        assert!(!broken.matches("correct horse"));
        assert_eq!(check_new("short"), Err(HubError::PasswordTooShort { min_chars: 8 }));
        assert_eq!(check_new(&"a".repeat(257)), Err(HubError::PasswordTooLong { max_chars: 256 }));
        // Eight characters, not eight bytes.
        assert!(check_new("ééééééé").is_err());
        assert!(check_new("éééééééé").is_ok());
    }

    #[test]
    fn the_record_never_shows_its_salt_or_hash() {
        let record = PasswordRecord { salt: vec![0xAB; 16], rounds: 5, hash: vec![0xCD; 32] };
        let text = format!("{record:?}");
        assert!(!text.contains("171") && !text.contains("ab") && !text.contains("205"), "{text}");
        assert!(text.contains("hidden"), "{text}");
    }
}
