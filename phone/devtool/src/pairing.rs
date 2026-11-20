//! The pairing file: what `setup` hands LiMo on the phone, as the README
//! fixes it ("What LiMo's debug build must do"). One JSON object, with nothing after it:
//!
//! ```text
//! {"welcome":{...the core's Grant...},"code_secret_hex":"<64 hex digits>","made_at":<unix seconds>,"child_name":"<name>"}
//! ```
//!
//! - `welcome`: the welcome grant the hub made at pairing, as the core's own
//!   JSON. It holds the child's rules and the guardian PIN's record, which
//!   is the core's salted slow hash of the PIN, never the PIN.
//! - `code_secret_hex`: the phone's 32 byte code secret, in lowercase hex.
//!   One time codes are made from it, so it is the file's real secret.
//! - `made_at`: when this PC made it, by this PC's clock.
//! - `child_name`: the child's name, as the test family keeps it (see
//!   [`crate::family::clean_child_name`]): 1 to 40 characters. The welcome
//!   carries none, and the phone has no other source for it.
//!
//! The bytes are made into memory that is wiped when it is dropped, and go
//! to the phone only through adb's standard input, into LiMo's own files,
//! which LiMo deletes once it has taken them.

#![forbid(unsafe_code)]

use limo_core::Grant;
use limo_hub::Pairing;
use serde::Serialize;
use zeroize::Zeroizing;

/// The file's fields, in their order.
#[derive(Serialize)]
struct PairingFile<'a> {
    welcome: &'a Grant,
    code_secret_hex: &'a str,
    made_at: i64,
    child_name: &'a str,
}

/// Lowercase hex of some bytes, into memory that is wiped when dropped.
pub fn hex(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// The pairing file's bytes for this pairing, made at `made_at`, for the
/// child of this name.
pub fn pairing_bytes(pairing: &Pairing, made_at: i64, child_name: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let secret = hex(&pairing.code_secret);
    let file = PairingFile { welcome: &pairing.welcome, code_secret_hex: &secret, made_at, child_name };
    // Room made at once, so that the buffer never grows and leaves a copy.
    let mut bytes = Zeroizing::new(Vec::with_capacity(256 * 1024));
    serde_json::to_writer(&mut *bytes, &file).map_err(|e| format!("The pairing file could not be written out: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_two_digits_a_byte() {
        assert_eq!(*hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(*hex(&[]), "");
    }
}
