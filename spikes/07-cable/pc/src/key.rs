//! This program's own adb key: an RSA key of 2048 bits, as adb's own, made
//! once and kept in the user's own folder, so that the phone's "Always allow
//! from this computer" holds from one run to the next.
//!
//! adb_client makes a new random key, and keeps it nowhere, whenever the
//! file it is given does not exist; the phone would then ask again at every
//! run. So the key is made here, and the file handed to adb_client.
//!
//! The key is not Google's adb's (`.android\adbkey` in the user's folder),
//! so the phone asks for it on its own, and allowing it allows this program
//! only.
//!
//! It lives in `%LOCALAPPDATA%\LiMo-spike07-TEST-ONLY\adbkey` on Windows
//! (elsewhere the same folder in `$XDG_DATA_HOME` or `~/.local/share`),
//! readable by the user and administrators as that folder is. Deleting the
//! folder forgets it; the phone's "Revoke USB debugging authorisations"
//! forgets it on the phone.

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rsa::rand_core::OsRng;
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey};

/// The folder's name, which says what it is.
pub const FOLDER: &str = "LiMo-spike07-TEST-ONLY";
/// The key file's name in it.
pub const FILE: &str = "adbkey";
/// How the folder is named on screen, never with this PC's own path.
pub const SHOWN_AS: &str = "%LOCALAPPDATA%\\LiMo-spike07-TEST-ONLY\\adbkey";

/// The size of adb's keys.
const BITS: usize = 2048;
/// The words of a 2048 bit modulus, as Android's public key format counts them.
const WORDS: u32 = 64;

/// The key this program uses.
#[derive(Debug, Clone)]
pub struct Key {
    /// Where its file is.
    pub path: PathBuf,
    /// True when it was made in this run (the phone has never seen it).
    pub new: bool,
    /// Its fingerprint, as the phone shows it on "Allow USB debugging?".
    pub fingerprint: String,
}

/// The folder the key lives in, in the user's own folder.
pub fn folder() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(FOLDER));
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .map(|d| d.join(FOLDER))
}

/// The key in `dir`, made there first if there is none.
pub fn load_or_make(dir: &Path) -> Result<Key, String> {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(pem) => {
            let key = RsaPrivateKey::from_pkcs8_pem(&pem)
                .map_err(|_| format!("The key file ({SHOWN_AS}) cannot be read as a key. Delete that folder and run this again."))?;
            Ok(Key { path, new: false, fingerprint: fingerprint(&key) })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir).map_err(|e| format!("The key's folder could not be made: {e}."))?;
            let key = RsaPrivateKey::new(&mut OsRng, BITS).map_err(|e| format!("No key could be made: {e}."))?;
            let pem = key.to_pkcs8_pem(LineEnding::LF).map_err(|e| format!("The key could not be written out: {e}."))?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| format!("The key file could not be made: {e}."))?;
            file.write_all(pem.as_bytes()).map_err(|e| format!("The key file could not be written: {e}."))?;
            Ok(Key { path, new: true, fingerprint: fingerprint(&key) })
        }
        Err(e) => Err(format!("The key file ({SHOWN_AS}) cannot be read: {e}.")),
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

/// The fingerprint Android shows: the MD5 of the public key in its own
/// format, as pairs of uppercase hex digits joined by colons (Android's
/// `AdbDebuggingManager`).
pub fn fingerprint(key: &RsaPrivateKey) -> String {
    let blob = android_public_key(key.n(), key.e());
    Md5::digest(&blob).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("limo-spike07-key-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_key_is_made_once_and_kept() {
        let dir = temp_dir("kept");
        let first = load_or_make(&dir).unwrap();
        assert!(first.new);
        let again = load_or_make(&dir).unwrap();
        assert!(!again.new);
        assert_eq!(first.fingerprint, again.fingerprint);
        assert_eq!(first.fingerprint.len(), 16 * 3 - 1);
        assert!(first.fingerprint.split(':').all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())));
        let pem = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"), "PKCS#8, which adb_client reads");
        std::fs::write(dir.join(FILE), "not a key").unwrap();
        assert!(load_or_make(&dir).unwrap_err().contains("Delete that folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_public_key_holds_what_android_reads() {
        let key = RsaPrivateKey::new(&mut OsRng, BITS).unwrap();
        let blob = android_public_key(key.n(), key.e());
        assert_eq!(blob.len(), 524, "Android's RSAPublicKey: 4 + 4 + 256 + 256 + 4 bytes");
        assert_eq!(u32::from_le_bytes(blob[0..4].try_into().unwrap()), 64);
        let n0inv = u32::from_le_bytes(blob[4..8].try_into().unwrap());
        let n0 = u32::from_le_bytes(blob[8..12].try_into().unwrap());
        assert_eq!(n0.wrapping_mul(n0inv), u32::MAX, "n0inv is minus the inverse of n0 modulo 2^32");
        assert_eq!(BigUint::from_bytes_le(&blob[8..264]), key.n().clone());
        let rr = BigUint::from_bytes_le(&blob[264..520]);
        let two = BigUint::from(2u32);
        assert_eq!(rr, two.modpow(&BigUint::from(4096u32), key.n()), "R squared mod N, with R = 2^2048");
        assert_eq!(u32::from_le_bytes(blob[520..524].try_into().unwrap()), 65537);
    }
}
