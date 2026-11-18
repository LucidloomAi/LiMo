//! Random bytes, from the operating system, behind one small function.
//!
//! Ids, salts and code secrets all come from [`fill`]. In this crate's unit
//! tests a seeded generator can stand in for the operating system, so that a
//! test can make the same "random" bytes twice on purpose.

use crate::error::HubError;

/// Fills `buf` with random bytes.
pub(crate) fn fill(buf: &mut [u8]) -> Result<(), HubError> {
    #[cfg(test)]
    if seeded::fill(buf) {
        return Ok(());
    }
    getrandom::fill(buf).map_err(|_| HubError::NoRandomness)
}

/// `n` random bytes.
pub(crate) fn bytes(n: usize) -> Result<Vec<u8>, HubError> {
    let mut out = vec![0u8; n];
    fill(&mut out)?;
    Ok(out)
}

/// A new id: the prefix, then 16 lowercase hex digits (64 random bits).
pub(crate) fn id(prefix: &str) -> Result<String, HubError> {
    let mut raw = [0u8; 8];
    fill(&mut raw)?;
    let mut id = String::with_capacity(prefix.len() + 16);
    id.push_str(prefix);
    for b in raw {
        id.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        id.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    Ok(id)
}

#[cfg(test)]
pub(crate) mod seeded {
    //! A stand in for the operating system's randomness, for unit tests.

    use std::cell::Cell;

    thread_local! {
        static STATE: Cell<Option<u64>> = const { Cell::new(None) };
    }

    /// From now on, on this thread, random bytes come from a generator
    /// started at `seed`; None goes back to the operating system.
    pub(crate) fn seed(seed: Option<u64>) {
        STATE.with(|s| s.set(seed));
    }

    /// Fills `buf` from the seeded generator, if one is set. SplitMix64.
    pub(crate) fn fill(buf: &mut [u8]) -> bool {
        STATE.with(|s| {
            let Some(mut state) = s.get() else { return false };
            for chunk in buf.chunks_mut(8) {
                state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                let mut z = state;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                z ^= z >> 31;
                chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
            }
            s.set(Some(state));
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_have_their_prefix_and_sixteen_hex_digits() {
        let a = id("child-").unwrap();
        assert!(a.starts_with("child-") && a.len() == 22, "{a}");
        assert!(a[6..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{a}");
        assert_ne!(a, id("child-").unwrap());
    }

    #[test]
    fn a_seed_makes_the_same_bytes_again() {
        seeded::seed(Some(7));
        let first = bytes(40).unwrap();
        seeded::seed(Some(7));
        assert_eq!(bytes(40).unwrap(), first);
        seeded::seed(None);
        assert_ne!(bytes(40).unwrap(), first);
    }
}
