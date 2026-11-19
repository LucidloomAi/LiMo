//! Keeping the log readable when something goes on happening: the first time
//! is logged at once, and after that at most once in a while, with a count
//! of the times held back.

#![forbid(unsafe_code)]

use std::collections::HashMap;

/// The most different things remembered before starting again empty.
const MAX_KEYS: usize = 1_024;

#[derive(Debug, Clone, Copy)]
struct Entry {
    logged_at: u64,
    held: u64,
}

/// Remembers what was logged, and when.
#[derive(Debug)]
pub struct Quiet {
    every_ms: u64,
    entries: HashMap<String, Entry>,
}

impl Quiet {
    /// Logs each thing at most once every `every_ms` milliseconds.
    pub fn new(every_ms: u64) -> Quiet {
        Quiet { every_ms, entries: HashMap::new() }
    }

    /// The line to log for `key` now, or None while it is held back. A line
    /// logged after some were held back says how many.
    pub fn line(&mut self, key: &str, text: &str, now_ms: u64) -> Option<String> {
        match self.entries.get_mut(key) {
            None => {
                if self.entries.len() >= MAX_KEYS {
                    self.entries.clear();
                }
                self.entries.insert(key.to_string(), Entry { logged_at: now_ms, held: 0 });
                Some(text.to_string())
            }
            Some(entry) if now_ms.saturating_sub(entry.logged_at) >= self.every_ms => {
                let held = entry.held;
                *entry = Entry { logged_at: now_ms, held: 0 };
                Some(match held {
                    0 => text.to_string(),
                    1 => format!("{text} (once more since it was last logged)"),
                    n => format!("{text} ({n} times more since it was last logged)"),
                })
            }
            Some(entry) => {
                entry.held = entry.held.saturating_add(1);
                None
            }
        }
    }

    /// Forgets `key`, so that the next time it happens it is logged at once.
    pub fn forget(&mut self, key: &str) {
        self.entries.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_time_is_logged_then_once_in_a_while_with_a_count() {
        let mut quiet = Quiet::new(60_000);
        assert_eq!(quiet.line("a", "It broke.", 0).as_deref(), Some("It broke."));
        for t in 1..60 {
            assert_eq!(quiet.line("a", "It broke.", t * 1_000), None);
        }
        assert_eq!(quiet.line("b", "Other.", 30_000).as_deref(), Some("Other."));
        assert_eq!(
            quiet.line("a", "It broke.", 60_000).as_deref(),
            Some("It broke. (59 times more since it was last logged)")
        );
        assert_eq!(quiet.line("a", "It broke.", 60_001), None);
        assert_eq!(
            quiet.line("a", "It broke.", 120_000).as_deref(),
            Some("It broke. (once more since it was last logged)")
        );
        quiet.forget("a");
        assert_eq!(quiet.line("a", "It broke.", 120_001).as_deref(), Some("It broke."));
    }
}
