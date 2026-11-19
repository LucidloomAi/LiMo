//! Hex and base32, written out here because they are a few lines each.
//!
//! Hex is lowercase, two characters a byte. Base32 is the alphabet of RFC
//! 4648 section 6 (`A` to `Z`, then `2` to `7`), without padding: it is
//! read in either case, and only in its one canonical form, with the unused
//! bits of the last character zero. The known answers of RFC 4648 section
//! 10 are tests below.

use zeroize::{Zeroize, Zeroizing};

const HEX: &[u8; 16] = b"0123456789abcdef";
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Bytes as lowercase hex, in a string that is wiped when dropped.
pub(crate) fn hex(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Hex into exactly `N` bytes, or None. The bytes are wiped when dropped.
pub(crate) fn unhex<const N: usize>(text: &str) -> Option<Zeroizing<[u8; N]>> {
    let text = text.as_bytes();
    if text.len() != N * 2 {
        return None;
    }
    let mut out = Zeroizing::new([0u8; N]);
    for (i, pair) in text.as_chunks::<2>().0.iter().enumerate() {
        out[i] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Some(out)
}

/// Hex into bytes of any length, or None. The bytes are wiped when dropped.
pub(crate) fn unhex_vec(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() / 2));
    for pair in text.as_chunks::<2>().0 {
        out.push((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?);
    }
    Some(out)
}

/// Bytes as base32, uppercase, without padding, in a string that is wiped
/// when dropped.
pub(crate) fn base32(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(bytes.len().div_ceil(5) * 8));
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(BASE32[((acc >> bits) & 31) as usize]));
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(BASE32[((acc << (5 - bits)) & 31) as usize]));
    }
    acc.zeroize();
    out
}

/// Why base32 text could not be read. Only where, never what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Base32Problem {
    /// A character outside the alphabet, at this place (counted from 1).
    BadCharacter(usize),
    /// A length no bytes can have, or unused bits that are not zero.
    NotCanonical,
}

/// Base32, in either case, into bytes wiped when dropped.
pub(crate) fn unbase32(text: &str) -> Result<Zeroizing<Vec<u8>>, Base32Problem> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a'),
        b'2'..=b'7' => Some(c - b'2' + 26),
        _ => None,
    };
    if let Some(at) = text.bytes().position(|c| value(c).is_none()) {
        return Err(Base32Problem::BadCharacter(at + 1));
    }
    // 1, 3 and 6 characters leave bits over that are not a whole byte.
    if matches!(text.len() % 8, 1 | 3 | 6) {
        return Err(Base32Problem::NotCanonical);
    }
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in text.bytes() {
        acc = (acc << 5) | u32::from(value(c).unwrap_or(0));
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
        acc &= (1 << bits) - 1;
    }
    let leftover_is_zero = acc == 0;
    acc.zeroize();
    if !leftover_is_zero {
        return Err(Base32Problem::NotCanonical);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_both_ways() {
        assert_eq!(&*hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(*unhex::<4>("000fa5ff").unwrap(), [0x00, 0x0f, 0xa5, 0xff]);
        assert_eq!(*unhex::<4>("000FA5FF").unwrap(), [0x00, 0x0f, 0xa5, 0xff]);
        assert!(unhex::<4>("000fa5f").is_none());
        assert!(unhex::<4>("000fa5ffff").is_none());
        assert!(unhex::<2>("0g00").is_none());
        assert!(unhex::<2>("+100").is_none());
        assert_eq!(&*unhex_vec("").unwrap(), &[] as &[u8]);
        assert_eq!(&*unhex_vec("0102").unwrap(), &[1, 2]);
        assert!(unhex_vec("010").is_none());
        assert!(unhex_vec("01 2").is_none());
    }

    #[test]
    fn base32_known_answers_of_rfc_4648() {
        for (plain, coded) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(&*base32(plain.as_bytes()), coded);
            assert_eq!(&*unbase32(coded).unwrap(), plain.as_bytes());
            assert_eq!(&*unbase32(&coded.to_lowercase()).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn base32_round_trips_every_length() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        for n in 0..bytes.len() {
            let coded = base32(&bytes[..n]);
            assert_eq!(&*unbase32(&coded).unwrap(), &bytes[..n], "{n}");
        }
    }

    #[test]
    fn base32_refuses_what_is_not_canonical() {
        // Padding, spaces and characters outside the alphabet.
        assert_eq!(unbase32("MY======"), Err(Base32Problem::BadCharacter(3)));
        assert_eq!(unbase32("MZ XQ"), Err(Base32Problem::BadCharacter(3)));
        assert_eq!(unbase32("MZXQ1"), Err(Base32Problem::BadCharacter(5)));
        assert_eq!(unbase32("MZXQ8"), Err(Base32Problem::BadCharacter(5)));
        // Lengths no bytes have.
        assert_eq!(unbase32("M"), Err(Base32Problem::NotCanonical));
        assert_eq!(unbase32("MZX"), Err(Base32Problem::NotCanonical));
        assert_eq!(unbase32("MZXW6Y"), Err(Base32Problem::NotCanonical));
        // "MZ" is "f" with zero unused bits; "M3" is "f" with one set.
        assert_eq!(unbase32("M3"), Err(Base32Problem::NotCanonical));
    }
}
