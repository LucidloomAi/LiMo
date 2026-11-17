//! The client side: send one HELLO to an address and check the reply. Used by
//! `hello` (no discovery) and by `browse`.

use crate::clock;
use crate::out;
use crate::protocol::{self, LineRead};
use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// How long connecting may take. The whole exchange is capped at
/// protocol::IO_LIMIT.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(3);

/// The `hello` command. Exit code 0 when the hub answered properly.
pub fn run(host: &str, port: u16) -> u8 {
    let addr = match resolve(host, port) {
        Ok(a) => a,
        Err(e) => {
            out::line(format!(
                "hello failed host={host} port={port} error={}",
                out::quote(&e)
            ));
            return 1;
        }
    };
    if say_hello(addr, host) {
        0
    } else {
        1
    }
}

/// An address for host:port, IPv4 first. A literal address needs no lookup.
fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("could not look up the host: {e}"))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
        .ok_or_else(|| "the host has no address".to_string())
}

/// Sends one HELLO, prints one `hello ok` or `hello failed` line, and says
/// whether it worked. `label` is the host as the user or mDNS gave it.
pub fn say_hello(addr: SocketAddr, label: &str) -> bool {
    let host = if label.is_empty() {
        addr.ip().to_string()
    } else {
        label.to_string()
    };
    let port = addr.port();
    match exchange(addr) {
        Ok(done) => {
            out::line(format!(
                "hello ok host={host} port={port} rtt_ms={} connect_ms={} reply={}",
                clock::ms_one_decimal(done.rtt),
                clock::ms_one_decimal(done.connect),
                protocol::printable(&done.reply)
            ));
            true
        }
        Err(e) => {
            out::line(format!(
                "hello failed host={host} port={port} error={}",
                out::quote(&e)
            ));
            false
        }
    }
}

struct Done {
    /// From the start of connecting to the reply's line end.
    rtt: Duration,
    /// From the start of connecting to connected.
    connect: Duration,
    reply: Vec<u8>,
}

fn exchange(addr: SocketAddr) -> Result<Done, String> {
    let nonce = protocol::new_token();
    let start = Instant::now();
    let deadline = start + protocol::IO_LIMIT;
    let mut stream =
        TcpStream::connect_timeout(&addr, CONNECT_LIMIT).map_err(|e| connect_words(&e))?;
    let connect = start.elapsed();
    let left = clock::remaining(deadline, Instant::now()).max(Duration::from_millis(1));
    let _ = stream.set_write_timeout(Some(left));
    stream
        .write_all(protocol::hello_line(&nonce).as_bytes())
        .map_err(|e| format!("could not send the HELLO: {e}"))?;
    let read = protocol::read_line_capped(&mut stream, deadline);
    let rtt = start.elapsed();
    let _ = stream.shutdown(Shutdown::Both);
    match read {
        LineRead::Line(reply) => match protocol::check_reply(&reply, &nonce) {
            Ok(_) => Ok(Done {
                rtt,
                connect,
                reply,
            }),
            Err(why) => Err(format!("{why}: {}", protocol::printable(&reply))),
        },
        LineRead::TooLong => Err("the reply was longer than 256 bytes".to_string()),
        LineRead::Closed(p) if p.is_empty() => {
            Err("no reply: the connection closed with nothing sent back".to_string())
        }
        LineRead::Closed(p) => Err(format!(
            "the connection closed before a line end: {}",
            protocol::printable(&p)
        )),
        LineRead::TimedOut(_) => Err(format!(
            "connected, but no reply within {} s",
            protocol::IO_LIMIT.as_secs()
        )),
        LineRead::Failed(e, _) => Err(format!("reading the reply failed: {e}")),
    }
}

/// A failed connect in plain words, with the system's own text after it.
fn connect_words(e: &io::Error) -> String {
    let what = match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => format!(
            "timed out while connecting ({} s): nothing answered, so a firewall or the router may be dropping it, or no computer has that address",
            CONNECT_LIMIT.as_secs()
        ),
        io::ErrorKind::ConnectionRefused => {
            "connection refused: the computer answered, but nothing listens on that port, or a firewall rejected it".to_string()
        }
        io::ErrorKind::PermissionDenied => {
            "this computer's own rules refused the connection".to_string()
        }
        io::ErrorKind::HostUnreachable | io::ErrorKind::NetworkUnreachable => {
            "no route: this computer has no way to reach that address".to_string()
        }
        _ => "could not connect".to_string(),
    };
    format!("{what} [{e}]")
}
