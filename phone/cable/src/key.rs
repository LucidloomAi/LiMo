//! LiMo's own adb key: an RSA key of 2048 bits, as adb's own, made once and
//! kept for the guardian's Windows account, in `%LOCALAPPDATA%\LiMo\adbkey`,
//! so that the phone's "Always allow from this computer" holds from one
//! Add a phone to the next. The folder is the account's own, readable by
//! that account, the system and administrators only: the child's account
//! cannot read it.
//!
//! The phone knows a PC by this key: allowing it once, with "Always allow",
//! lets LiMo on this PC (in this Windows account) reach the phone over the
//! cable without asking again. It is not Google's adb's key, and LiMo never
//! reads or uses that one.
//!
//! What the phone is shown is the key's public half in Android's own format
//! ([`AdbKey::public_message`]); "Allow USB debugging?" shows its
//! fingerprint ([`AdbKey::fingerprint`]), which the console may show too, so
//! that a guardian can see the two are the same.

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rsa::rand_core::OsRng;
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey};
use zeroize::Zeroizing;

/// The folder in the account's local application data.
pub const FOLDER: &str = "LiMo";
/// The key file's name in it.
pub const FILE: &str = "adbkey";
/// The name the key carries on the phone (adb's own carry `user@host`; this
/// names no person and no PC).
pub const KEY_NAME: &str = "limo@limo";

/// The size of adb's keys.
const BITS: usize = 2048;
/// The words of a 2048 bit modulus, as Android's public key format counts them.
const WORDS: u32 = 64;

/// Why the key could not be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// No folder for it: the account has no local application data.
    NoFolder,
    /// The file is there but holds no key LiMo can read.
    Damaged,
    /// The file could not be read or written; the system's words.
    Io(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::NoFolder => f.write_str("This Windows account has no folder for LiMo's key for phones (LOCALAPPDATA is not set)."),
            KeyError::Damaged => f.write_str(
                "LiMo's key for phones on this PC cannot be read. A phone that allowed this PC before will ask again once it is made anew.",
            ),
            KeyError::Io(why) => write!(f, "LiMo's key for phones could not be read or kept: {why}."),
        }
    }
}

/// A key adb speaks with. Its `Debug` shows only its fingerprint; the key is
/// wiped from memory when dropped (the `rsa` crate's own key does so).
#[derive(Clone)]
pub struct AdbKey {
    key: RsaPrivateKey,
    /// True when it was made just now: no phone has seen it.
    pub new: bool,
}

impl std::fmt::Debug for AdbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AdbKey({}, new {})", self.fingerprint(), self.new)
    }
}

/// The folder LiMo's key lives in, for this Windows account.
pub fn default_folder() -> Result<PathBuf, KeyError> {
    let base = if cfg!(windows) { std::env::var_os("LOCALAPPDATA") } else { std::env::var_os("XDG_DATA_HOME") };
    base.filter(|b| !b.is_empty()).map(|b| PathBuf::from(b).join(FOLDER)).ok_or(KeyError::NoFolder)
}

impl AdbKey {
    /// LiMo's key in the default folder, made there first if there is none.
    pub fn for_this_account() -> Result<AdbKey, KeyError> {
        AdbKey::load_or_make(&default_folder()?)
    }

    /// The key in `folder`, made there first if there is none. A file that
    /// is there but damaged is never replaced here: that is the caller's to
    /// decide ([`AdbKey::replace`]), since a phone that allowed the old key
    /// asks again for the new one.
    pub fn load_or_make(folder: &Path) -> Result<AdbKey, KeyError> {
        let path = folder.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => AdbKey::from_pem(&Zeroizing::new(text)).ok_or(KeyError::Damaged),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match AdbKey::make(folder, &path) {
                // Another caller made it in the same moment: read theirs, once
                // it is whole.
                Err(KeyError::Io(_)) if path.exists() => AdbKey::read_when_whole(&path),
                made => made,
            },
            Err(e) => Err(KeyError::Io(e.to_string())),
        }
    }

    fn read_when_whole(path: &Path) -> Result<AdbKey, KeyError> {
        for _ in 0..40 {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Some(key) = AdbKey::from_pem(&Zeroizing::new(text))
            {
                return Ok(key);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err(KeyError::Damaged)
    }

    /// Makes a new key in `folder` in place of one that is damaged: the old
    /// file is kept beside it as `adbkey.damaged`.
    pub fn replace(folder: &Path) -> Result<AdbKey, KeyError> {
        let path = folder.join(FILE);
        if path.exists() {
            std::fs::rename(&path, folder.join(format!("{FILE}.damaged"))).map_err(|e| KeyError::Io(e.to_string()))?;
        }
        AdbKey::make(folder, &path)
    }

    fn make(folder: &Path, path: &Path) -> Result<AdbKey, KeyError> {
        std::fs::create_dir_all(folder).map_err(|e| KeyError::Io(e.to_string()))?;
        let key = RsaPrivateKey::new(&mut OsRng, BITS).map_err(|e| KeyError::Io(e.to_string()))?;
        let pem = key.to_pkcs8_pem(LineEnding::LF).map_err(|e| KeyError::Io(e.to_string()))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| KeyError::Io(e.to_string()))?;
        file.write_all(pem.as_bytes()).map_err(|e| KeyError::Io(e.to_string()))?;
        Ok(AdbKey { key, new: true })
    }

    /// A key from its PEM text: PKCS#8, as LiMo and adb write keys, or
    /// PKCS#1. None when the text holds no such key.
    pub fn from_pem(text: &str) -> Option<AdbKey> {
        RsaPrivateKey::from_pkcs8_pem(text)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(text))
            .ok()
            .map(|key| AdbKey { key, new: false })
    }

    /// The key itself, for signing.
    pub(crate) fn private(&self) -> &RsaPrivateKey {
        &self.key
    }

    /// The fingerprint Android shows on "Allow USB debugging?": the MD5 of
    /// the public key in its own format, as pairs of uppercase hex digits
    /// joined by colons (Android's `AdbDebuggingManager`).
    pub fn fingerprint(&self) -> String {
        let blob = android_public_key(self.key.n(), self.key.e());
        Md5::digest(&blob).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
    }

    /// What the phone is shown: the public key in Android's own format, in
    /// base64, a space, the key's name, and a zero.
    pub fn public_message(&self) -> Vec<u8> {
        let mut out = format!("{} {KEY_NAME}", base64(&android_public_key(self.key.n(), self.key.e()))).into_bytes();
        out.push(0);
        out
    }
}

/// The key's public half in Android's own format (`android_pubkey_encode` in
/// AOSP's `libcrypto_utils`): the number of words, n0inv, the modulus and
/// R squared mod N (both little end first), then the exponent.
pub fn android_public_key(n: &BigUint, e: &BigUint) -> Vec<u8> {
    let size = BITS / 8;
    let modulus = padded_le(n, size);
    let n0 = u32::from_le_bytes([modulus[0], modulus[1], modulus[2], modulus[3]]);
    // The inverse of n0 modulo 2^32 by Newton's iteration: n0 is odd, so n0
    // is its own inverse modulo 8, and each step doubles the bits that are
    // right (3, 6, 12, 24, 48).
    let mut inverse = n0;
    for _ in 0..5 {
        inverse = inverse.wrapping_mul(2u32.wrapping_sub(n0.wrapping_mul(inverse)));
    }
    let n0inv = inverse.wrapping_neg();
    let rr = (BigUint::from(1u32) << (2 * BITS)) % n;
    let exponent = padded_le(e, 4);
    let mut out = Vec::with_capacity(4 + 4 + size + size + 4);
    out.extend_from_slice(&WORDS.to_le_bytes());
    out.extend_from_slice(&n0inv.to_le_bytes());
    out.extend_from_slice(&modulus);
    out.extend_from_slice(&padded_le(&rr, size));
    out.extend_from_slice(&exponent[..4]);
    out
}

fn padded_le(value: &BigUint, size: usize) -> Vec<u8> {
    let mut bytes = value.to_bytes_le();
    bytes.resize(size.max(bytes.len()), 0);
    bytes
}

/// RFC 4648 base64, standard alphabet, with padding.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("limo-cable-key-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_key_is_made_once_and_kept_and_a_damaged_one_is_never_replaced_unasked() {
        let dir = temp_dir("kept");
        let first = AdbKey::load_or_make(&dir).unwrap();
        assert!(first.new);
        let again = AdbKey::load_or_make(&dir).unwrap();
        assert!(!again.new);
        assert_eq!(first.fingerprint(), again.fingerprint());
        assert_eq!(first.fingerprint().len(), 16 * 3 - 1);
        let pem = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"), "PKCS#8, as adb writes its own");
        std::fs::write(dir.join(FILE), "not a key").unwrap();
        assert_eq!(AdbKey::load_or_make(&dir).unwrap_err(), KeyError::Damaged);
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "not a key", "left as it was");
        let replaced = AdbKey::replace(&dir).unwrap();
        assert!(replaced.new);
        assert_eq!(std::fs::read_to_string(dir.join("adbkey.damaged")).unwrap(), "not a key");
        assert_eq!(AdbKey::load_or_make(&dir).unwrap().fingerprint(), replaced.fingerprint());
        assert!(format!("{replaced:?}").starts_with("AdbKey("), "its Debug shows the fingerprint only");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_public_key_holds_what_android_reads() {
        let key = AdbKey { key: RsaPrivateKey::new(&mut OsRng, BITS).unwrap(), new: true };
        let blob = android_public_key(key.key.n(), key.key.e());
        assert_eq!(blob.len(), 524, "Android's RSAPublicKey: 4 + 4 + 256 + 256 + 4 bytes");
        assert_eq!(u32::from_le_bytes(blob[0..4].try_into().unwrap()), 64);
        let n0inv = u32::from_le_bytes(blob[4..8].try_into().unwrap());
        let n0 = u32::from_le_bytes(blob[8..12].try_into().unwrap());
        assert_eq!(n0.wrapping_mul(n0inv), u32::MAX, "n0inv is minus the inverse of n0 modulo 2^32");
        assert_eq!(BigUint::from_bytes_le(&blob[8..264]), key.key.n().clone());
        let rr = BigUint::from_bytes_le(&blob[264..520]);
        assert_eq!(rr, BigUint::from(2u32).modpow(&BigUint::from(4096u32), key.key.n()), "R squared mod N, with R = 2^2048");
        assert_eq!(u32::from_le_bytes(blob[520..524].try_into().unwrap()), 65537);
        let message = key.public_message();
        assert_eq!(message.last(), Some(&0));
        assert!(String::from_utf8_lossy(&message).ends_with(" limo@limo\0"));
    }

    #[test]
    fn base64_is_rfc_4648_with_padding_and_both_pem_forms_are_read() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(plain.as_bytes()), coded);
        }
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
        use rsa::pkcs1::EncodeRsaPrivateKey;
        let key = RsaPrivateKey::new(&mut OsRng, BITS).unwrap();
        let pkcs1 = key.to_pkcs1_pem(LineEnding::LF).unwrap();
        assert_eq!(AdbKey::from_pem(&pkcs1).unwrap().key, key);
        assert!(AdbKey::from_pem("nothing").is_none());
    }
}
