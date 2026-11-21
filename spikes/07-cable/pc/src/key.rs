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
//!
//! **For this proof only**, the WinUSB route can instead use the adb key of
//! this Windows account, Google's adb's own (`--key adb`), which a phone may
//! have allowed long ago, so that a step runs with nobody at the phone
//! ([`google_adb`]). That key is only read: never written, copied or moved,
//! never printed, and its place never said beyond "the adb key of this
//! Windows account". The real product makes its own key, and the guardian
//! allows it once.

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
/// How Google's adb's key is named on screen: never its path.
pub const GOOGLE_SHOWN_AS: &str = "the adb key of this Windows account";

/// The size of adb's keys.
const BITS: usize = 2048;
/// The words of a 2048 bit modulus, as Android's public key format counts them.
const WORDS: u32 = 64;

/// Whose key it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// This program's own, made once and kept.
    Own,
    /// Google's adb's own key of this Windows account, read only, for this
    /// proof alone (`--key adb`).
    GoogleAdb,
}

/// The key this program uses. Its `Debug` leaves out where the file is.
#[derive(Clone)]
pub struct Key {
    /// Where its file is: shown only for this program's own key, and then as
    /// [`SHOWN_AS`], never as this PC's path.
    pub path: PathBuf,
    /// True when it was made in this run (the phone has never seen it).
    pub new: bool,
    /// Its fingerprint, as the phone shows it on "Allow USB debugging?".
    pub fingerprint: String,
    /// Whose it is.
    pub source: KeySource,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({:?}, new {}, fingerprint {})", self.source, self.new, self.fingerprint)
    }
}

impl Key {
    /// The key itself, read from its file (read only).
    pub fn load(&self) -> Result<RsaPrivateKey, String> {
        match self.source {
            KeySource::Own => private_key(&self.path),
            KeySource::GoogleAdb => read_google_key(&self.path),
        }
    }

    /// How it is named on screen.
    pub fn shown_as(&self) -> &'static str {
        match self.source {
            KeySource::Own => SHOWN_AS,
            KeySource::GoogleAdb => GOOGLE_SHOWN_AS,
        }
    }
}

/// Where Google's adb keeps the key of this Windows account: in the folder
/// `ANDROID_USER_HOME` names, else in `.android` in the user's own folder.
pub fn google_adb_key_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ANDROID_USER_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join(FILE));
    }
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") };
    home.map(|h| PathBuf::from(h).join(".android").join(FILE))
}

/// Reads Google's adb's key at `path`, read only: PKCS#8, as adb writes it,
/// or PKCS#1. The text is wiped from memory once read.
fn read_google_key(path: &Path) -> Result<RsaPrivateKey, String> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    let pem = zeroize::Zeroizing::new(std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "This Windows account has no adb key (Google's adb makes one the first time it runs). Run this without --key adb."
                .to_string()
        } else {
            format!("{GOOGLE_SHOWN_AS} cannot be read: {e}.")
        }
    })?);
    RsaPrivateKey::from_pkcs8_pem(&pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&pem))
        .map_err(|_| format!("{GOOGLE_SHOWN_AS} cannot be read as a key."))
}

/// Google's adb's key of this Windows account, for this proof's
/// `--key adb`: read only, its fingerprint as the phone shows it.
pub fn google_adb() -> Result<Key, String> {
    let path = google_adb_key_path().ok_or_else(|| format!("No folder is set for {GOOGLE_SHOWN_AS}."))?;
    google_adb_at(path)
}

/// The same, from a given file: for the tests.
pub fn google_adb_at(path: PathBuf) -> Result<Key, String> {
    let key = read_google_key(&path)?;
    Ok(Key { fingerprint: fingerprint(&key), path, new: false, source: KeySource::GoogleAdb })
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
            Ok(Key { path, new: false, fingerprint: fingerprint(&key), source: KeySource::Own })
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
            Ok(Key { path, new: true, fingerprint: fingerprint(&key), source: KeySource::Own })
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

/// The private key in the file at `path`, for the WinUSB route, which
/// speaks adb itself.
pub fn private_key(path: &Path) -> Result<RsaPrivateKey, String> {
    let pem = std::fs::read_to_string(path).map_err(|e| format!("The key file ({SHOWN_AS}) cannot be read: {e}."))?;
    RsaPrivateKey::from_pkcs8_pem(&pem)
        .map_err(|_| format!("The key file ({SHOWN_AS}) cannot be read as a key. Delete that folder and run this again."))
}

/// The public key as adb hands it to the phone: Android's own format, in
/// standard base64 with padding.
pub fn public_key_base64(key: &RsaPrivateKey) -> String {
    base64(&android_public_key(key.n(), key.e()))
}

/// The name this program gives its key on the phone (adb's own gives
/// `user@host`; this names no person and no PC).
pub const KEY_NAME: &str = "limo-cable-spike@limo";

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
    fn googles_adb_key_is_read_only_in_either_format_and_never_shown_by_its_path() {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        let dir = temp_dir("google");
        std::fs::create_dir_all(&dir).unwrap();
        let key = RsaPrivateKey::new(&mut OsRng, BITS).unwrap();
        for (name, pem) in [
            ("pkcs8", key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string()),
            ("pkcs1", key.to_pkcs1_pem(LineEnding::LF).unwrap().to_string()),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, &pem).unwrap();
            let before = std::fs::metadata(&path).unwrap().modified().unwrap();
            let found = google_adb_at(path.clone()).unwrap();
            assert_eq!(found.source, KeySource::GoogleAdb);
            assert_eq!(found.fingerprint, fingerprint(&key));
            assert_eq!(found.load().unwrap(), key);
            assert_eq!(found.shown_as(), "the adb key of this Windows account");
            assert!(!format!("{found:?}").contains(name), "its Debug form leaves out the path");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), pem, "never written");
            assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
        }
        let missing = google_adb_at(dir.join("none")).unwrap_err();
        assert!(missing.starts_with("This Windows account has no adb key"));
        assert!(!missing.contains(&dir.display().to_string()), "the path is never said");
        std::fs::write(dir.join("bad"), "not a key").unwrap();
        assert!(google_adb_at(dir.join("bad")).unwrap_err().contains("cannot be read as a key"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_is_rfc_4648_with_padding() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(plain.as_bytes()), coded);
        }
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
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
