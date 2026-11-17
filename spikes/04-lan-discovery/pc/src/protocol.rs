//! The tiny text protocol. One line each way:
//!
//!   client: `HELLO <nonce>\n`
//!   server: `LIMO <id> <version> <unix time> <nonce>\n`, then it closes.
//!
//! Anything else gets `ERR\n`. A line, counting its line end, is at most 256
//! bytes, and a connection gets 5 seconds in all. Everything here except
//! `impl LineSource for TcpStream` is pure and unit tested.

use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// The DNS-SD service type, as mdns-sd wants it.
pub const SERVICE_TYPE: &str = "_limo-spike._tcp.local.";
/// The longest line in either direction, counting its line end.
pub const MAX_LINE: usize = 256;
/// The whole time a connection may take, on both sides.
pub const IO_LIMIT: Duration = Duration::from_secs(5);
/// The longest nonce.
pub const MAX_NONCE: usize = 64;

/// What came in before the line end, the cap, the close or the time limit.
#[derive(Debug, PartialEq, Eq)]
pub enum LineRead {
    /// A whole line, without its line end.
    Line(Vec<u8>),
    /// MAX_LINE bytes arrived and none of them was a line end.
    TooLong,
    /// The other side closed first. Holds what came before.
    Closed(Vec<u8>),
    /// The time ran out first. Holds what came before.
    TimedOut(Vec<u8>),
    /// Reading failed. The error text, and what came before.
    Failed(String, Vec<u8>),
}

/// Something a line can be read from with a time limit on each read.
pub trait LineSource {
    fn set_timeout(&mut self, limit: Duration) -> io::Result<()>;
    fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

impl LineSource for TcpStream {
    fn set_timeout(&mut self, limit: Duration) -> io::Result<()> {
        self.set_read_timeout(Some(limit))
    }
    fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read(buf)
    }
}

/// Reads up to the first line end, never holding more than MAX_LINE bytes,
/// and gives up at `deadline`.
pub fn read_line_capped<S: LineSource>(src: &mut S, deadline: Instant) -> LineRead {
    let mut buf: Vec<u8> = Vec::with_capacity(MAX_LINE);
    let mut chunk = [0u8; MAX_LINE];
    loop {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            buf.truncate(pos);
            return LineRead::Line(buf);
        }
        if buf.len() >= MAX_LINE {
            return LineRead::TooLong;
        }
        let now = Instant::now();
        if now >= deadline {
            return LineRead::TimedOut(buf);
        }
        if let Err(e) = src.set_timeout(deadline - now) {
            return LineRead::Failed(e.to_string(), buf);
        }
        let room = MAX_LINE - buf.len();
        match src.read_some(&mut chunk[..room]) {
            Ok(0) => return LineRead::Closed(buf),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return LineRead::TimedOut(buf)
            }
            Err(e) => return LineRead::Failed(e.to_string(), buf),
        }
    }
}

/// A nonce is 1 to 64 characters, each an ASCII letter, a digit, '-' or '_'.
/// That keeps it safe to echo back and to print.
pub fn is_valid_nonce(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NONCE
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Strips one trailing carriage return, so a CRLF line end also works.
fn without_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// The nonce, if the line is exactly `HELLO <nonce>`.
pub fn parse_hello(line: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(without_cr(line)).ok()?;
    let nonce = text.strip_prefix("HELLO ")?;
    if is_valid_nonce(nonce) {
        Some(nonce.to_string())
    } else {
        None
    }
}

pub fn hello_line(nonce: &str) -> String {
    format!("HELLO {nonce}\n")
}

pub fn limo_line(id: &str, version: &str, unix_time: u64, nonce: &str) -> String {
    format!("LIMO {id} {version} {unix_time} {nonce}\n")
}

pub const ERR_LINE: &str = "ERR\n";

/// What the server sends back, and how to describe what it received.
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    /// The reply line, with its line end.
    pub reply: String,
    /// For the event line: the received text quoted, or what happened.
    pub received: String,
}

pub fn answer(read: &LineRead, id: &str, version: &str, unix_time: u64) -> Answer {
    let partial = |p: &[u8]| {
        if p.is_empty() {
            String::new()
        } else {
            format!(" partial={}", printable(p))
        }
    };
    match read {
        LineRead::Line(bytes) => {
            let reply = match parse_hello(bytes) {
                Some(nonce) => limo_line(id, version, unix_time, &nonce),
                None => ERR_LINE.to_string(),
            };
            Answer {
                reply,
                received: printable(without_cr(bytes)),
            }
        }
        LineRead::TooLong => Answer {
            reply: ERR_LINE.to_string(),
            received: format!("(over {MAX_LINE} bytes with no line end)"),
        },
        LineRead::Closed(p) => Answer {
            reply: ERR_LINE.to_string(),
            received: format!("(closed before a line end){}", partial(p)),
        },
        LineRead::TimedOut(p) => Answer {
            reply: ERR_LINE.to_string(),
            received: format!(
                "(no line end within {} s){}",
                IO_LIMIT.as_secs(),
                partial(p)
            ),
        },
        LineRead::Failed(e, p) => Answer {
            reply: ERR_LINE.to_string(),
            received: format!("(read failed: {e}){}", partial(p)),
        },
    }
}

/// A parsed `LIMO` reply.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: String,
    pub version: String,
    pub unix_time: u64,
    pub nonce: String,
}

/// Checks a reply line (without its line end) against the nonce that was
/// sent. The error is in plain words.
pub fn check_reply(line: &[u8], sent_nonce: &str) -> Result<Reply, String> {
    let line = without_cr(line);
    let text = std::str::from_utf8(line).map_err(|_| "the reply is not text".to_string())?;
    if text == "ERR" {
        return Err("the hub answered ERR".to_string());
    }
    let parts: Vec<&str> = text.split(' ').collect();
    if parts.len() != 5 || parts[0] != "LIMO" {
        return Err("the reply is not a LIMO line".to_string());
    }
    if parts[1].is_empty() || parts[2].is_empty() {
        return Err("the reply has an empty id or version".to_string());
    }
    let unix_time: u64 = parts[3]
        .parse()
        .map_err(|_| "the reply's time is not a number".to_string())?;
    if parts[4] != sent_nonce {
        return Err("the reply's nonce does not match the one sent".to_string());
    }
    Ok(Reply {
        id: parts[1].to_string(),
        version: parts[2].to_string(),
        unix_time,
        nonce: parts[4].to_string(),
    })
}

/// Bytes from the network as a quoted, printable string for the log: ASCII
/// printable characters kept, a double quote or backslash escaped, anything
/// else as \xNN.
pub fn printable(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    s.push('"');
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\x{b:02x}")),
        }
    }
    s.push('"');
    s
}

/// A random 8 character lowercase hex token (the server id, a nonce).
pub fn new_token() -> String {
    format!("{:08x}", fastrand::u32(..))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A scripted source: each item is what one read returns.
    struct Script {
        steps: VecDeque<io::Result<Vec<u8>>>,
        reads: usize,
    }

    impl Script {
        fn new(steps: Vec<io::Result<Vec<u8>>>) -> Self {
            Script {
                steps: steps.into(),
                reads: 0,
            }
        }
    }

    impl LineSource for Script {
        fn set_timeout(&mut self, limit: Duration) -> io::Result<()> {
            assert!(limit > Duration::ZERO);
            Ok(())
        }
        fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            match self.steps.pop_front() {
                None => Ok(0),
                Some(Err(e)) => Err(e),
                Some(Ok(mut data)) => {
                    let n = data.len().min(buf.len());
                    buf[..n].copy_from_slice(&data[..n]);
                    if n < data.len() {
                        let rest = data.split_off(n);
                        self.steps.push_front(Ok(rest));
                    }
                    Ok(n)
                }
            }
        }
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    fn read(steps: Vec<io::Result<Vec<u8>>>) -> LineRead {
        read_line_capped(&mut Script::new(steps), soon())
    }

    // reading lines

    #[test]
    fn reads_one_line_in_one_piece() {
        assert_eq!(
            read(vec![Ok(b"HELLO abc\n".to_vec())]),
            LineRead::Line(b"HELLO abc".to_vec())
        );
    }

    #[test]
    fn reads_one_line_in_many_pieces() {
        assert_eq!(
            read(vec![
                Ok(b"HEL".to_vec()),
                Ok(b"LO a".to_vec()),
                Ok(b"bc\nextra".to_vec())
            ]),
            LineRead::Line(b"HELLO abc".to_vec())
        );
    }

    #[test]
    fn line_of_exactly_256_bytes_with_its_line_end_is_accepted() {
        let mut data = vec![b'a'; MAX_LINE - 1];
        data.push(b'\n');
        assert_eq!(read(vec![Ok(data)]), LineRead::Line(vec![b'a'; MAX_LINE - 1]));
    }

    #[test]
    fn oversized_input_is_cut_off_at_256_bytes() {
        let mut src = Script::new(vec![Ok(vec![b'a'; 10_000])]);
        assert_eq!(read_line_capped(&mut src, soon()), LineRead::TooLong);
        // It never asked for more than the cap.
        assert_eq!(src.reads, 1);
        assert_eq!(src.steps.front().map(|s| s.as_ref().unwrap().len()), Some(10_000 - MAX_LINE));
    }

    #[test]
    fn oversized_input_in_pieces_is_cut_off_too() {
        let steps = (0..100).map(|_| Ok(vec![b'x'; 100])).collect();
        assert_eq!(read(steps), LineRead::TooLong);
    }

    #[test]
    fn close_before_line_end() {
        assert_eq!(read(vec![]), LineRead::Closed(vec![]));
        assert_eq!(
            read(vec![Ok(b"HELLO abc".to_vec())]),
            LineRead::Closed(b"HELLO abc".to_vec())
        );
    }

    #[test]
    fn time_out_keeps_what_came() {
        assert_eq!(
            read(vec![
                Ok(b"HEL".to_vec()),
                Err(io::Error::new(io::ErrorKind::TimedOut, "t"))
            ]),
            LineRead::TimedOut(b"HEL".to_vec())
        );
        assert_eq!(
            read(vec![Err(io::Error::new(io::ErrorKind::WouldBlock, "w"))]),
            LineRead::TimedOut(vec![])
        );
    }

    #[test]
    fn deadline_already_passed() {
        let past = Instant::now();
        let mut src = Script::new(vec![Ok(b"HELLO a\n".to_vec())]);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(read_line_capped(&mut src, past), LineRead::TimedOut(vec![]));
        assert_eq!(src.reads, 0);
    }

    #[test]
    fn interrupted_is_retried_and_other_errors_fail() {
        assert_eq!(
            read(vec![
                Err(io::Error::new(io::ErrorKind::Interrupted, "i")),
                Ok(b"HELLO a\n".to_vec())
            ]),
            LineRead::Line(b"HELLO a".to_vec())
        );
        match read(vec![
            Ok(b"HE".to_vec()),
            Err(io::Error::new(io::ErrorKind::ConnectionReset, "reset"))
        ]) {
            LineRead::Failed(e, p) => {
                assert!(e.contains("reset"));
                assert_eq!(p, b"HE".to_vec());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    // the hello line

    #[test]
    fn parses_a_good_hello() {
        assert_eq!(parse_hello(b"HELLO 1a2b3c4d"), Some("1a2b3c4d".to_string()));
        assert_eq!(parse_hello(b"HELLO a"), Some("a".to_string()));
        assert_eq!(parse_hello(b"HELLO A-z_9"), Some("A-z_9".to_string()));
        assert_eq!(parse_hello(b"HELLO abc\r"), Some("abc".to_string()));
        let longest = "n".repeat(MAX_NONCE);
        assert_eq!(
            parse_hello(format!("HELLO {longest}").as_bytes()),
            Some(longest)
        );
    }

    #[test]
    fn rejects_malformed_hellos() {
        for bad in [
            &b""[..],
            b"HELLO",
            b"HELLO ",
            b"hello abc",
            b"HELLO  abc",
            b"HELLO abc def",
            b"HELLO abc ",
            b" HELLO abc",
            b"HELLOabc",
            b"HELO abc",
            b"HELLO ab.c",
            b"HELLO ab\"c",
            b"HELLO \xff\xfe",
            b"HELLO abc\r\r",
            b"GET / HTTP/1.1",
        ] {
            assert_eq!(parse_hello(bad), None, "accepted {:?}", printable(bad));
        }
        let too_long = format!("HELLO {}", "n".repeat(MAX_NONCE + 1));
        assert_eq!(parse_hello(too_long.as_bytes()), None);
    }

    // the answer

    #[test]
    fn answers_a_good_hello() {
        let a = answer(
            &LineRead::Line(b"HELLO 1a2b3c4d".to_vec()),
            "ab12cd34",
            "0.1.0",
            1_795_000_000,
        );
        assert_eq!(a.reply, "LIMO ab12cd34 0.1.0 1795000000 1a2b3c4d\n");
        assert_eq!(a.received, "\"HELLO 1a2b3c4d\"");
    }

    #[test]
    fn answers_err_to_everything_else() {
        for read in [
            LineRead::Line(b"HELLO".to_vec()),
            LineRead::Line(b"GET / HTTP/1.1\r".to_vec()),
            LineRead::Line(vec![0, 1, 2]),
            LineRead::TooLong,
            LineRead::Closed(vec![]),
            LineRead::Closed(b"HELLO abc".to_vec()),
            LineRead::TimedOut(vec![]),
            LineRead::TimedOut(b"HEL".to_vec()),
            LineRead::Failed("reset".to_string(), vec![]),
        ] {
            assert_eq!(answer(&read, "id", "0.1.0", 1).reply, "ERR\n", "{read:?}");
        }
    }

    #[test]
    fn describes_what_was_received() {
        assert_eq!(
            answer(&LineRead::TooLong, "i", "v", 1).received,
            "(over 256 bytes with no line end)"
        );
        assert_eq!(
            answer(&LineRead::TimedOut(vec![]), "i", "v", 1).received,
            "(no line end within 5 s)"
        );
        assert_eq!(
            answer(&LineRead::TimedOut(b"HE".to_vec()), "i", "v", 1).received,
            "(no line end within 5 s) partial=\"HE\""
        );
        assert_eq!(
            answer(&LineRead::Closed(vec![]), "i", "v", 1).received,
            "(closed before a line end)"
        );
        assert_eq!(
            answer(&LineRead::Line(b"\x00bad\"".to_vec()), "i", "v", 1).received,
            "\"\\x00bad\\\"\""
        );
    }

    #[test]
    fn the_longest_reply_fits_in_a_line() {
        let r = limo_line(
            &new_token(),
            env!("CARGO_PKG_VERSION"),
            u64::MAX,
            &"n".repeat(MAX_NONCE),
        );
        assert!(r.len() <= MAX_LINE, "{} bytes", r.len());
        assert!(hello_line(&"n".repeat(MAX_NONCE)).len() <= MAX_LINE);
    }

    // the client's check of the reply

    #[test]
    fn checks_a_good_reply() {
        assert_eq!(
            check_reply(b"LIMO ab12cd34 0.1.0 1795000000 n1", "n1"),
            Ok(Reply {
                id: "ab12cd34".to_string(),
                version: "0.1.0".to_string(),
                unix_time: 1_795_000_000,
                nonce: "n1".to_string()
            })
        );
        assert!(check_reply(b"LIMO ab12cd34 0.1.0 1795000000 n1\r", "n1").is_ok());
    }

    #[test]
    fn rejects_bad_replies() {
        assert_eq!(
            check_reply(b"ERR", "n1"),
            Err("the hub answered ERR".to_string())
        );
        assert!(check_reply(b"LIMO ab12cd34 0.1.0 1795000000 other", "n1")
            .unwrap_err()
            .contains("nonce"));
        assert!(check_reply(b"LIMO ab12cd34 0.1.0 soon n1", "n1").is_err());
        assert!(check_reply(b"LIMO ab12cd34 0.1.0 1795000000", "n1").is_err());
        assert!(check_reply(b"LIMO ab12cd34 0.1.0 1795000000 n1 extra", "n1").is_err());
        assert!(check_reply(b"HELLO n1", "n1").is_err());
        assert!(check_reply(b"LIMO  0.1.0 1 n1", "n1").is_err());
        assert!(check_reply(b"\xff", "n1").is_err());
        assert!(check_reply(b"", "n1").is_err());
    }

    #[test]
    fn tokens_are_eight_hex_characters() {
        for _ in 0..50 {
            let t = new_token();
            assert_eq!(t.len(), 8);
            assert!(t.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(is_valid_nonce(&t));
        }
    }
}
