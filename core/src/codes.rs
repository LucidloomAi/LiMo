//! One time codes: time added on a device that cannot reach the master.
//!
//! A guardian who is not at home reads a short code over a phone call; the
//! child types it on the lock screen; the device adds time. Neither side needs
//! a network.
//!
//! Each device has its own **code secret**: 32 random bytes made by the
//! caller when the device is paired (this library has no randomness), kept by
//! the master and by the device, and handed by the master to the guardian
//! phones while they are at home.
//!
//! A code is 8 decimal digits, shown as two groups of four: "1342 0715".
//!
//! - The first two digits are the **selector**: the minutes it adds (0 for 15
//!   minutes, 1 for 30, 2 for 60) times ten, plus which code of the day it is
//!   (0 to 9). So 13 is the fourth code of the day, worth 30 minutes. The
//!   selector is not secret.
//! - The last six digits are taken from HMAC-SHA256, keyed with the device's
//!   code secret, over a version label, the device id, the home day and the
//!   selector (see [`make_code`] for the exact bytes). They are the secret
//!   part: without the code secret, a guess is right once in a million.
//!
//! So a code works only on the device it was made for and only on the home
//! day it was made for. The device remembers which selectors it has used that
//! day, so each code works once. The time it adds is recorded with an id made
//! from the device, the day and the selector, so the master counts it once
//! however often the device reports it.

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ctutils::CtEq;

/// The minutes a code can add: 15, 30 or 60. Index 0, 1 and 2 in the
/// selector.
pub const CODE_MINUTES: [u32; 3] = [15, 30, 60];

/// How many codes of each kind a device can take in one home day: counters
/// 0 to 9.
pub const CODES_PER_DAY: u8 = 10;

/// The length of a code secret, in bytes.
pub const CODE_SECRET_BYTES: u32 = 32;

/// The version label at the start of every message a code is made from.
/// Changing it, or anything else in the construction, changes every code; the
/// known answer tests below catch that.
const LABEL: &[u8] = b"LiMo one time code v1";

/// Bytes that must stay private: a code secret. Kept by the master and by a
/// device. Its `Debug` says only how long it is.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Secret(Vec<u8>);

impl Secret {
    pub(crate) fn new(bytes: Vec<u8>) -> Secret {
        Secret(bytes)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn is_set(&self) -> bool {
        !self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "Secret(none)")
        } else {
            write!(f, "Secret(hidden, {} bytes)", self.0.len())
        }
    }
}

/// A one time code, made on a guardian's phone (or at the master) for one
/// device and one home day. Until it is used it is a secret, so its `Debug`
/// hides the digits.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimeCode {
    /// The 8 digits, without a space: "13420715".
    pub digits: String,
    /// The same, as two groups of four for reading out: "1342 0715".
    pub shown: String,
    /// The device it works on.
    pub device_id: String,
    /// The home day it works on.
    pub day: i64,
    /// The minutes it adds: 15, 30 or 60.
    pub minutes: u32,
    /// Which code of the day it is, 0 to 9.
    pub counter: u8,
    /// The first two digits: the minutes' index times ten plus the counter.
    pub selector: u8,
}

impl fmt::Debug for OneTimeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OneTimeCode {{ device_id: {:?}, day: {}, minutes: {}, counter: {}, selector: {}, digits: hidden }}",
            self.device_id, self.day, self.minutes, self.counter, self.selector
        )
    }
}

/// Why a code could not be made, or a code secret was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeError {
    /// A code secret must be exactly 32 bytes.
    SecretWrongLength {
        /// The length given, in bytes.
        given: u32,
    },
    /// Codes add 15, 30 or 60 minutes, nothing else.
    MinutesNotOffered {
        /// The minutes asked for.
        minutes: u32,
    },
    /// The counter must be from 0 to 9.
    CounterOutOfRange {
        /// The counter given.
        counter: u8,
    },
    /// All ten codes of this kind have been made for this device and day.
    AllCodesMade {
        /// The home day.
        day: i64,
        /// The minutes asked for.
        minutes: u32,
    },
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeError::SecretWrongLength { given } => write!(
                f,
                "A code secret must be {CODE_SECRET_BYTES} bytes long; this one is {given} bytes."
            ),
            CodeError::MinutesNotOffered { minutes } => {
                write!(f, "A code adds 15, 30 or 60 minutes, not {minutes}.")
            }
            CodeError::CounterOutOfRange { counter } => {
                write!(f, "A code counter runs from 0 to 9, not {counter}.")
            }
            CodeError::AllCodesMade { minutes, .. } => write!(
                f,
                "All ten codes of {minutes} minutes for this device have been made today. Give time from home instead, or wait for tomorrow."
            ),
        }
    }
}

impl std::error::Error for CodeError {}

/// Checks that a code secret is 32 bytes.
pub(crate) fn check_secret(secret: &[u8]) -> Result<(), CodeError> {
    if secret.len() as u64 == CODE_SECRET_BYTES as u64 {
        Ok(())
    } else {
        Err(CodeError::SecretWrongLength { given: u32::try_from(secret.len()).unwrap_or(u32::MAX) })
    }
}

/// The selector for `minutes` and `counter`, or why there is none.
fn selector_for(minutes: u32, counter: u8) -> Result<u8, CodeError> {
    let index = CODE_MINUTES
        .iter()
        .position(|m| *m == minutes)
        .ok_or(CodeError::MinutesNotOffered { minutes })?;
    if counter >= CODES_PER_DAY {
        return Err(CodeError::CounterOutOfRange { counter });
    }
    Ok(index as u8 * 10 + counter)
}

/// The minutes a selector stands for, or None when no code has it.
pub(crate) fn minutes_of_selector(selector: u8) -> Option<u32> {
    CODE_MINUTES.get(usize::from(selector / 10)).copied()
}

/// The six secret digits of the code for `selector`, as ASCII digits.
fn secret_digits(secret: &[u8], device_id: &str, day: i64, selector: u8) -> [u8; 6] {
    // HMAC takes a key of any length, so this never fails. A failure would
    // give a code no one can type.
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return *b"xxxxxx";
    };
    mac.update(LABEL);
    mac.update(&[0]);
    mac.update(&(device_id.len() as u64).to_be_bytes());
    mac.update(device_id.as_bytes());
    mac.update(&day.to_be_bytes());
    mac.update(&[selector]);
    let out = mac.finalize().into_bytes();
    let mut first = [0u8; 8];
    first.copy_from_slice(&out[..8]);
    let mut n = u64::from_be_bytes(first) % 1_000_000;
    let mut digits = [b'0'; 6];
    for d in digits.iter_mut().rev() {
        *d = b'0' + (n % 10) as u8;
        n /= 10;
    }
    digits
}

/// Makes the code for one device, home day, number of minutes and counter.
///
/// The six secret digits are HMAC-SHA256, keyed with the 32 byte code
/// secret, over these bytes in order:
///
/// 1. the label `LiMo one time code v1` (21 bytes of ASCII), then one zero
///    byte;
/// 2. the length of the device id in bytes, as 8 bytes big-endian, then the
///    device id in UTF-8;
/// 3. the home day, as a signed 8 byte big-endian number;
/// 4. the selector, one byte.
///
/// The first 8 bytes of the result, read as a big-endian number, modulo one
/// million, written with leading zeros, are the six digits. The code is the
/// selector's two digits followed by those six.
pub fn make_code(
    secret: Vec<u8>,
    device_id: &str,
    day: i64,
    minutes: u32,
    counter: u8,
) -> Result<OneTimeCode, CodeError> {
    check_secret(&secret)?;
    let selector = selector_for(minutes, counter)?;
    let tail = secret_digits(&secret, device_id, day, selector);
    let mut digits = format!("{selector:02}");
    digits.extend(tail.iter().map(|b| char::from(*b)));
    let shown = format!("{} {}", &digits[..4], &digits[4..]);
    Ok(OneTimeCode { digits, shown, device_id: device_id.to_string(), day, minutes, counter, selector })
}

/// Makes the next code for a device and day: the lowest counter not in
/// `counters_made`, the counters of the codes already made for that device
/// that day, of any minutes. Running out of counters is an error to show, not
/// a panic.
///
/// Every guardian phone that makes codes must know the counters the others
/// used that day, or two of them can make the same code (the second one would
/// then be refused as used). Phones that cannot share can split the counters
/// between them and pass the ones they must not use in `counters_made`.
pub fn next_code(
    secret: Vec<u8>,
    device_id: &str,
    day: i64,
    minutes: u32,
    counters_made: Vec<u8>,
) -> Result<OneTimeCode, CodeError> {
    check_secret(&secret)?;
    selector_for(minutes, 0)?;
    let counter = (0..CODES_PER_DAY)
        .find(|c| !counters_made.contains(c))
        .ok_or(CodeError::AllCodesMade { day, minutes })?;
    make_code(secret, device_id, day, minutes, counter)
}

/// The 8 digits of a typed code, with spaces and hyphens taken out, or None
/// when it is not 8 digits. A code that is not well formed is refused without
/// counting as a wrong guess.
pub fn code_digits(typed: &str) -> Option<String> {
    let digits: String = typed.chars().filter(|c| *c != ' ' && *c != '-').collect();
    (digits.len() == 8 && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits)
}

/// What a typed code is, for this device on this day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeCheck {
    /// Not 8 digits.
    Malformed,
    /// Well formed, but not a code for this device and day.
    Wrong,
    /// Right.
    Right {
        /// Its selector.
        selector: u8,
        /// The minutes it adds.
        minutes: u32,
    },
}

/// Checks a typed code against the code secret, the device and the day. The
/// secret digits are compared in constant time.
pub(crate) fn check_code(secret: &[u8], device_id: &str, day: i64, typed: &str) -> CodeCheck {
    let Some(digits) = code_digits(typed) else {
        return CodeCheck::Malformed;
    };
    let bytes = digits.as_bytes();
    let selector = (bytes[0] - b'0') * 10 + (bytes[1] - b'0');
    let Some(minutes) = minutes_of_selector(selector) else {
        return CodeCheck::Wrong;
    };
    let expected = secret_digits(secret, device_id, day, selector);
    if expected[..].ct_eq(&bytes[2..]).to_bool() {
        CodeCheck::Right { selector, minutes }
    } else {
        CodeCheck::Wrong
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> Vec<u8> {
        (0u8..32).collect()
    }

    /// Known answers, worked out separately with Python's `hmac` and
    /// `hashlib` from the description in [`make_code`]. If one of these
    /// changes, every code a guardian phone makes changes with it.
    #[test]
    fn known_answers() {
        let cases = [
            ("phone-1", 20_775, 15, 0, "00844681"),
            ("phone-1", 20_775, 30, 3, "13916020"),
            ("phone-1", 20_775, 60, 9, "29949127"),
            ("phone-1", 20_774, 15, 0, "00898424"),
            ("pc", 20_775, 15, 0, "00984179"),
            ("phone-1", -1, 15, 5, "05122310"),
        ];
        for (device, day, minutes, counter, digits) in cases {
            let code = make_code(secret(), device, day, minutes, counter).unwrap();
            assert_eq!(code.digits, digits, "{device} {day} {minutes} {counter}");
            assert_eq!(code.shown, format!("{} {}", &digits[..4], &digits[4..]));
            assert_eq!(check_code(&secret(), device, day, &code.shown), CodeCheck::Right {
                selector: code.selector,
                minutes
            });
        }
    }

    #[test]
    fn a_code_works_only_for_its_device_and_day() {
        let code = make_code(secret(), "phone-1", 20_775, 30, 3).unwrap();
        assert_eq!(check_code(&secret(), "phone-2", 20_775, &code.digits), CodeCheck::Wrong);
        assert_eq!(check_code(&secret(), "phone-1", 20_776, &code.digits), CodeCheck::Wrong);
        let mut other = secret();
        other[0] ^= 1;
        assert_eq!(check_code(&other, "phone-1", 20_775, &code.digits), CodeCheck::Wrong);
        // Changing the selector changes the secret digits too.
        let moved = format!("23{}", &code.digits[2..]);
        assert_eq!(check_code(&secret(), "phone-1", 20_775, &moved), CodeCheck::Wrong);
    }

    #[test]
    fn typed_codes_are_read_leniently_but_must_be_eight_digits() {
        assert_eq!(code_digits("1342 0715"), Some("13420715".into()));
        assert_eq!(code_digits(" 1342-0715 "), Some("13420715".into()));
        for bad in ["", "1342 071", "1342 07155", "1342 07a5", "１３４２０７１５", "1342\t0715"] {
            assert_eq!(code_digits(bad), None, "{bad:?}");
            assert_eq!(check_code(&secret(), "phone-1", 1, bad), CodeCheck::Malformed);
        }
        // A selector no code has is well formed, and simply wrong.
        assert_eq!(check_code(&secret(), "phone-1", 1, "3100 0000"), CodeCheck::Wrong);
        assert_eq!(check_code(&secret(), "phone-1", 1, "9999 9999"), CodeCheck::Wrong);
    }

    #[test]
    fn the_next_code_takes_the_lowest_free_counter_and_runs_out_politely() {
        let code = next_code(secret(), "phone-1", 7, 60, vec![0, 1, 3]).unwrap();
        assert_eq!((code.counter, code.selector), (2, 22));
        let all: Vec<u8> = (0..10).collect();
        let err = next_code(secret(), "phone-1", 7, 15, all).unwrap_err();
        assert_eq!(err, CodeError::AllCodesMade { day: 7, minutes: 15 });
        assert!(err.to_string().ends_with('.'));
        assert_eq!(make_code(secret(), "p", 7, 20, 0).unwrap_err(), CodeError::MinutesNotOffered { minutes: 20 });
        assert_eq!(make_code(secret(), "p", 7, 15, 10).unwrap_err(), CodeError::CounterOutOfRange { counter: 10 });
        assert_eq!(make_code(vec![1; 16], "p", 7, 15, 0).unwrap_err(), CodeError::SecretWrongLength { given: 16 });
    }

    #[test]
    fn a_secret_never_shows_in_debug() {
        let code = make_code(secret(), "phone-1", 20_775, 30, 3).unwrap();
        let text = format!("{code:?}");
        assert!(!text.contains("1391") && !text.contains("6020") && text.contains("hidden"), "{text}");
        let s = Secret::new(vec![0xAB; 32]);
        let text = format!("{s:?}");
        assert!(!text.contains("171") && !text.contains("ab") && !text.contains("AB"), "{text}");
        assert_eq!(format!("{:?}", Secret::default()), "Secret(none)");
    }
}
