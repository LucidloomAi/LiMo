//! The guardian PIN: typed on the child's device by a guardian, with no
//! network.
//!
//! A PIN is 4 to 8 digits. It is never kept: the master keeps a
//! [`PinRecord`], a salted slow hash of it, and every device gets the record
//! through grants, with a version so that a changed PIN replaces the old one
//! ([`PinUpdate`]). A device with no record refuses PIN actions.
//!
//! The hash is PBKDF2-HMAC-SHA256 with [`PIN_ITERATIONS`] rounds and a salt of
//! 16 bytes or more made by the caller. A PIN has at most a hundred million
//! values, so anyone who has the record can try them all, slow hash or not. A
//! short PIN is protected mainly by the devices keeping the record private,
//! and on the device by the wait after wrong guesses; the hash only makes
//! trying them all slower.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ctutils::CtEq;

/// PBKDF2 rounds for a new PIN record: 200,000. A third of the 2023 OWASP
/// figure for passwords, chosen so that a check stays well under a second on
/// the slowest 2020 phones, which hash in software; a PIN has so few values
/// that more rounds would change little for anyone holding the record. About
/// 16 ms on a 2023 desktop. The README gives the measured and estimated
/// times. Each record carries its rounds, so this can change later.
pub const PIN_ITERATIONS: u32 = 200_000;
/// The most rounds a device accepts in a record, so that a damaged record
/// cannot keep the lock screen busy for minutes.
pub const MAX_PIN_ITERATIONS: u32 = 10_000_000;
/// The shortest PIN, in digits.
pub const MIN_PIN_DIGITS: u32 = 4;
/// The longest PIN, in digits.
pub const MAX_PIN_DIGITS: u32 = 8;
/// The shortest salt, in bytes.
pub const MIN_SALT_BYTES: u32 = 16;
/// The length of the hash in a record, in bytes.
pub const PIN_HASH_BYTES: u32 = 32;

/// The salted slow hash of a guardian PIN. Its `Debug` hides the salt and the
/// hash.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinRecord {
    /// The salt, 16 bytes or more, made by the caller.
    pub salt: Vec<u8>,
    /// PBKDF2 rounds.
    pub iterations: u32,
    /// PBKDF2-HMAC-SHA256 of the PIN's digits as ASCII, 32 bytes.
    pub hash: Vec<u8>,
}

impl fmt::Debug for PinRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PinRecord {{ iterations: {}, salt: hidden ({} bytes), hash: hidden ({} bytes) }}",
            self.iterations,
            self.salt.len(),
            self.hash.len()
        )
    }
}

/// The PIN as the master last set it, as grants carry it: a version that
/// grows with every change, and the record, or None when the guardian has
/// removed the PIN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinUpdate {
    /// Grows by one with every change of the PIN. A device keeps the highest.
    pub version: u32,
    /// The record, or None when there is no PIN any more.
    pub record: Option<PinRecord>,
}

/// What a right PIN may do on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinAction {
    /// Add minutes for today, as time added on the device: 1 to 1440.
    AddMinutes {
        /// The minutes to add.
        minutes: u32,
    },
    /// Keep the device open until this moment, whatever the budget or the
    /// blocked hours say (not for blocked apps): at most a day ahead.
    UnlockUntil {
        /// Unix seconds, later than now.
        until: i64,
    },
    /// Confirm "release this device". Only says the PIN was right; the
    /// service does the releasing.
    ConfirmRelease,
}

/// Why a PIN record could not be made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinError {
    /// A PIN is 4 to 8 digits, 0 to 9.
    NotFourToEightDigits,
    /// The salt must be 16 bytes or more.
    SaltTooShort {
        /// The length given, in bytes.
        given: u32,
    },
    /// The rounds must be from 1 to [`MAX_PIN_ITERATIONS`].
    IterationsOutOfRange {
        /// The rounds given.
        given: u32,
    },
    /// A record made elsewhere is not one this library would make: see
    /// [`PinRecord::is_sound`].
    NotSound,
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinError::NotFourToEightDigits => {
                write!(f, "A PIN is {MIN_PIN_DIGITS} to {MAX_PIN_DIGITS} digits, with nothing else in it.")
            }
            PinError::SaltTooShort { given } => {
                write!(f, "The salt must be {MIN_SALT_BYTES} bytes or more; this one is {given} bytes.")
            }
            PinError::IterationsOutOfRange { given } => write!(
                f,
                "A PIN record must use 1 to {MAX_PIN_ITERATIONS} rounds of hashing, not {given}."
            ),
            PinError::NotSound => write!(
                f,
                "The PIN record needs a salt of {MIN_SALT_BYTES} bytes or more, 1 to {MAX_PIN_ITERATIONS} rounds and a hash of {PIN_HASH_BYTES} bytes."
            ),
        }
    }
}

impl std::error::Error for PinError {}

/// True when `pin` is 4 to 8 ASCII digits. A PIN that is not well formed is
/// refused without counting as a wrong guess.
pub fn pin_is_well_formed(pin: &str) -> bool {
    let n = pin.len() as u64;
    (MIN_PIN_DIGITS as u64..=MAX_PIN_DIGITS as u64).contains(&n) && pin.bytes().all(|b| b.is_ascii_digit())
}

fn derive(pin: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(pin.as_bytes(), salt, iterations, &mut out);
    out
}

impl PinRecord {
    /// The record for `pin` with this salt, at the full cost
    /// ([`PIN_ITERATIONS`]). Slow on purpose: a tenth of a second or more.
    pub fn new(pin: &str, salt: Vec<u8>) -> Result<PinRecord, PinError> {
        PinRecord::with_iterations(pin, salt, PIN_ITERATIONS)
    }

    /// The record for `pin` with this salt and number of rounds. Use
    /// [`PinRecord::new`] in the product; fewer rounds are for tests.
    pub fn with_iterations(pin: &str, salt: Vec<u8>, iterations: u32) -> Result<PinRecord, PinError> {
        if !pin_is_well_formed(pin) {
            return Err(PinError::NotFourToEightDigits);
        }
        if (salt.len() as u64) < MIN_SALT_BYTES as u64 {
            return Err(PinError::SaltTooShort { given: u32::try_from(salt.len()).unwrap_or(u32::MAX) });
        }
        if iterations == 0 || iterations > MAX_PIN_ITERATIONS {
            return Err(PinError::IterationsOutOfRange { given: iterations });
        }
        let hash = derive(pin, &salt, iterations).to_vec();
        Ok(PinRecord { salt, iterations, hash })
    }

    /// True when the record is one this library would make: a salt of 16
    /// bytes or more, 1 to [`MAX_PIN_ITERATIONS`] rounds, a 32 byte hash. A
    /// device ignores a record that is not.
    pub fn is_sound(&self) -> bool {
        self.salt.len() as u64 >= MIN_SALT_BYTES as u64
            && self.iterations > 0
            && self.iterations <= MAX_PIN_ITERATIONS
            && self.hash.len() as u64 == PIN_HASH_BYTES as u64
    }

    /// True when `pin` is the PIN of this record. The hashes are compared in
    /// constant time. A PIN that is not well formed, or a record that is not
    /// sound, never matches.
    pub(crate) fn matches(&self, pin: &str) -> bool {
        if !pin_is_well_formed(pin) || !self.is_sound() {
            return false;
        }
        derive(pin, &self.salt, self.iterations)[..].ct_eq(self.hash.as_slice()).to_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> Vec<u8> {
        (100u8..116).collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Known answers from Python's `hashlib.pbkdf2_hmac("sha256", ...)`.
    #[test]
    fn known_answers() {
        let one = PinRecord::with_iterations("2468", salt(), 1).unwrap();
        assert_eq!(hex(&one.hash), "c70b220fe054d934fc825a8873f9262a67b397439705b8e653b20254faecd3a9");
        let thousand = PinRecord::with_iterations("2468", salt(), 1_000).unwrap();
        assert_eq!(hex(&thousand.hash), "c6c4eb90a32f9b5b1cdfe180a593963e437c55d498ddf71ccd540990af45fdc5");
        let full = PinRecord::new("90210417", salt()).unwrap();
        assert_eq!(full.iterations, 200_000);
        assert_eq!(hex(&full.hash), "9e63710894df1400f6fa1d8097db8273d55753e32ffb67455b9e0ae5fa3097bf");
        assert!(full.matches("90210417"));
        assert!(!full.matches("90210418"));
    }

    #[test]
    fn only_four_to_eight_digits_make_a_pin() {
        for good in ["0000", "1234", "12345678"] {
            assert!(pin_is_well_formed(good), "{good}");
        }
        for bad in ["", "123", "123456789", "12a4", " 1234", "1234 ", "١٢٣٤"] {
            assert!(!pin_is_well_formed(bad), "{bad:?}");
            assert_eq!(PinRecord::with_iterations(bad, salt(), 1), Err(PinError::NotFourToEightDigits));
        }
        assert_eq!(PinRecord::with_iterations("1234", vec![0; 15], 1), Err(PinError::SaltTooShort { given: 15 }));
        assert_eq!(
            PinRecord::with_iterations("1234", salt(), 0),
            Err(PinError::IterationsOutOfRange { given: 0 })
        );
        let record = PinRecord::with_iterations("1234", salt(), 1).unwrap();
        assert!(record.matches("1234"));
        assert!(!record.matches("12340"));
        let mut broken = record.clone();
        broken.hash.pop();
        assert!(!broken.is_sound() && !broken.matches("1234"));
    }

    #[test]
    fn the_record_never_shows_its_salt_or_hash() {
        let record = PinRecord::with_iterations("4321", vec![0xEE; 16], 1).unwrap();
        let text = format!("{record:?} {:?}", PinUpdate { version: 3, record: Some(record.clone()) });
        assert!(!text.contains("238") && !text.contains("ee") && !text.contains("4321"), "{text}");
        assert!(!text.contains(&hex(&record.hash)[..6]), "{text}");
        for err in [PinError::NotFourToEightDigits, PinError::SaltTooShort { given: 2 }] {
            assert!(err.to_string().ends_with('.'));
        }
    }
}
