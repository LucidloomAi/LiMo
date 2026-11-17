//! Output: one time stamped line per event, and quoting for values that may
//! hold spaces or odd characters.

use std::io::Write;

/// Prints "HH:MM:SS.mmm <text>" and flushes. A closed stdout is ignored.
pub fn line(text: impl AsRef<str>) {
    let stamp = crate::clock::stamp();
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{stamp} {}", text.as_ref());
    let _ = o.flush();
}

/// Prints text as is, with no time stamp (for `status` and the usage text).
pub fn plain(text: impl AsRef<str>) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{}", text.as_ref());
    let _ = o.flush();
}

/// Wraps a string in double quotes. A double quote or backslash gets a
/// backslash in front; control characters become \xNN (or \u{N} above 0xff).
/// Other characters, including non ASCII letters, are kept.
pub fn quote(s: &str) -> String {
    let mut q = String::with_capacity(s.len() + 2);
    q.push('"');
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            c if c.is_control() => {
                let n = c as u32;
                if n <= 0xff {
                    q.push_str(&format!("\\x{n:02x}"));
                } else {
                    q.push_str(&format!("\\u{{{n:x}}}"));
                }
            }
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote("plain"), "\"plain\"");
        assert_eq!(quote("LiMo spike on DEN"), "\"LiMo spike on DEN\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb\tc"), "\"a\\x0ab\\x09c\"");
        assert_eq!(quote("caf\u{e9}"), "\"caf\u{e9}\"");
        assert_eq!(quote(""), "\"\"");
    }
}
