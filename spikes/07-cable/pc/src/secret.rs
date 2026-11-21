//! The sample secret written into the test app's door: random, made fresh
//! for each run, shaped like a pairing offer (a short word, a colon, then
//! base32 of 91 random bytes, the size of an offer with two addresses, and a
//! line feed), never printed, never kept, wiped from memory when dropped.
//! Only its length and its check are shown, and the test app shows the same
//! two for what it received, so a person can see that every byte arrived
//! as it was sent.

#![forbid(unsafe_code)]

use rsa::rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// How many random bytes the sample carries: as many as an offer with two
/// addresses (32 + 8 + 32 + 8 + 2 + 1 + 8).
pub const RANDOM_BYTES: usize = 91;

/// The sample secret's bytes, wiped when dropped. Its `Debug` hides them.
pub struct Sample {
    bytes: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Sample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sample({} bytes, check {})", self.bytes.len(), self.check())
    }
}

impl Sample {
    /// A fresh sample from the system's random source.
    pub fn new() -> Sample {
        let mut random = Zeroizing::new(vec![0u8; RANDOM_BYTES]);
        OsRng.fill_bytes(&mut random);
        Sample::from_random(&random)
    }

    /// The sample made from the given random bytes (the tests give their own).
    pub fn from_random(random: &[u8]) -> Sample {
        let mut text = Zeroizing::new(Vec::with_capacity(8 + random.len() * 2));
        text.extend_from_slice(b"spike07:");
        base32_into(random, &mut text);
        text.push(b'\n');
        Sample { bytes: text }
    }

    /// The bytes, to hand to the phone's standard input.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes it is.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when it holds nothing (it never does).
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Its check, as the test app computes it for what it took.
    pub fn check(&self) -> String {
        check(&self.bytes)
    }
}

impl Default for Sample {
    fn default() -> Self {
        Sample::new()
    }
}

/// The first 8 hex digits, lowercase, of the SHA-256 of `bytes`: enough to
/// tell that two runs of bytes are the same, too little to say anything about
/// what they hold.
pub fn check(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// RFC 4648 base32, uppercase, no padding, as an offer's text uses.
fn base32_into(bytes: &[u8], out: &mut Vec<u8>) {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize]);
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base32(bytes: &[u8]) -> String {
        let mut out = Vec::new();
        base32_into(bytes, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn base32_is_rfc_4648_without_padding() {
        // RFC 4648 section 10, padding left off.
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(base32(b"fo"), "MZXQ");
        assert_eq!(base32(b"foo"), "MZXW6");
        assert_eq!(base32(b"foob"), "MZXW6YQ");
        assert_eq!(base32(b"fooba"), "MZXW6YTB");
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn the_sample_is_shaped_like_an_offer_and_ends_with_a_line_feed() {
        let sample = Sample::from_random(&[0u8; RANDOM_BYTES]);
        let text = String::from_utf8(sample.bytes().to_vec()).unwrap();
        assert!(text.starts_with("spike07:AAAA"));
        assert!(text.ends_with('\n'));
        assert_eq!(sample.len(), 8 + 146 + 1);
        assert!(text.trim_end().bytes().all(|b| b.is_ascii_graphic()));
        let fresh = Sample::new();
        assert_eq!(fresh.len(), sample.len());
        assert_ne!(fresh.check(), Sample::new().check(), "each sample is new");
    }

    #[test]
    fn the_check_is_the_start_of_sha_256_and_the_debug_form_hides_the_bytes() {
        // SHA-256 of "abc" begins ba7816bf.
        assert_eq!(check(b"abc"), "ba7816bf");
        let sample = Sample::from_random(&[7u8; RANDOM_BYTES]);
        let shown = format!("{sample:?}");
        assert!(shown.contains("155 bytes"));
        assert!(!shown.contains("spike07:"));
    }
}
