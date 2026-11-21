//! What the measurement of WebView2's own contact reads, and the one thing
//! it runs: a proxy on a loopback port that writes down the host names it
//! is asked to reach and sends nothing on.
//!
//! Two ways of seeing, used together by `examples/webview2_contacts.rs`:
//!
//! - **The connections.** Every TCP connection with a remote end owned by
//!   the console or any process it started (WebView2's browser, its
//!   renderer, its network service, its crash handler), read from Windows'
//!   own table many times a second, as any account may read it. This sees
//!   what was really opened, by address.
//! - **The host log.** WebView2 started with `--proxy-server` pointed at
//!   [`HostLog`], which reads the first line of each request, keeps the
//!   host and port it names (never a path, never a header), answers 403 and
//!   closes. So nothing reaches the network through it, and what WebView2
//!   wanted to reach is known by name.
//!
//! Neither sees what Windows sends for itself, or UDP (DNS lookups, which
//! the DNS client service makes on a program's behalf, and QUIC).

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A port another program on the development PC already serves: the host
/// log never takes it, even when Windows offers it.
pub const NEVER_PORT: u16 = 8081;

/// One TCP connection with a remote end.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Connection {
    /// The owning process.
    pub pid: u32,
    /// Its program's name.
    pub process: String,
    /// The remote address.
    pub remote: String,
    /// The remote port.
    pub remote_port: u16,
    /// The local port.
    pub local_port: u16,
    /// Windows' state number (5 is established).
    pub state: u32,
}

/// `root` and every process that descends from it, by id and program name.
#[cfg(windows)]
pub fn process_tree(root: u32) -> Vec<(u32, String)> {
    tree_of(root, &crate::win::net::processes())
}

/// The processes of `all` (id, parent, name) that are `root` or descend
/// from it.
pub fn tree_of(root: u32, all: &[(u32, u32, String)]) -> Vec<(u32, String)> {
    let mut found: Vec<(u32, String)> = all.iter().filter(|(pid, _, _)| *pid == root).map(|(p, _, n)| (*p, n.clone())).collect();
    loop {
        let before = found.len();
        for (pid, parent, name) in all {
            if *pid != *parent && found.iter().any(|(f, _)| f == parent) && !found.iter().any(|(f, _)| f == pid) {
                found.push((*pid, name.clone()));
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

/// The TCP connections with a remote end that the processes `tree` own.
#[cfg(windows)]
pub fn connections_of(tree: &[(u32, String)]) -> Vec<Connection> {
    let pids: Vec<u32> = tree.iter().map(|(p, _)| *p).collect();
    let name = |pid: u32| tree.iter().find(|(p, _)| *p == pid).map(|(_, n)| n.clone()).unwrap_or_default();
    crate::win::net::connections_of(&pids, &name)
}

/// One request the host log was asked to pass on, and refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// Milliseconds since the host log started.
    pub ms: u128,
    /// The request's method: CONNECT for HTTPS, GET or another for plain
    /// HTTP.
    pub method: String,
    /// The host name or address asked for.
    pub host: String,
    /// The port.
    pub port: u16,
}

/// The method, host and port a proxy request's first line names, and
/// nothing else of it: "CONNECT edge.example:443 HTTP/1.1" gives
/// ("CONNECT", "edge.example", 443); "GET http://a.example/x HTTP/1.1"
/// gives ("GET", "a.example", 80), the path left out.
pub fn host_of_request_line(line: &str) -> Option<(String, String, u16)> {
    let mut parts = line.split(' ');
    let method = parts.next()?.trim();
    let target = parts.next()?.trim();
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let (authority, default_port) = if method == "CONNECT" {
        (target, 443)
    } else if let Some(rest) = target.strip_prefix("http://") {
        (rest.split(['/', '?', '#']).next()?, 80)
    } else {
        (target.strip_prefix("https://")?.split(['/', '?', '#']).next()?, 443)
    };
    // Credentials in the authority are never kept.
    let authority = authority.rsplit('@').next()?;
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, rest) = v6.split_once(']')?;
        let port = rest.strip_prefix(':').map(str::parse).transpose().ok()?.unwrap_or(default_port);
        (h.to_string(), port)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        (h.to_string(), p.parse().ok()?)
    } else {
        (authority.to_string(), default_port)
    };
    if host.is_empty() || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b".-:_".contains(&b)) {
        return None;
    }
    Some((method.to_string(), host.to_ascii_lowercase(), port))
}

/// The host log: a proxy on 127.0.0.1 that refuses everything and writes
/// down what it was asked for.
pub struct HostLog {
    port: u16,
    asked: Arc<Mutex<Vec<Asked>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HostLog {
    /// Starts on a loopback port Windows picks, never [`NEVER_PORT`].
    pub fn start() -> std::io::Result<HostLog> {
        let listener = loop {
            let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))?;
            if l.local_addr()?.port() != NEVER_PORT {
                break l;
            }
        };
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let asked = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (asked2, stop2) = (Arc::clone(&asked), Arc::clone(&stop));
        let started = Instant::now();
        let thread = std::thread::Builder::new().name("limo host log".to_string()).spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, from)) if from.ip().is_loopback() => {
                        let asked3 = Arc::clone(&asked2);
                        let _ = std::thread::Builder::new()
                            .name("limo host log request".to_string())
                            .spawn(move || refuse(stream, started, &asked3));
                    }
                    Ok(_) => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        })?;
        Ok(HostLog { port, asked, stop, thread: Some(thread) })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Everything it was asked for so far.
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

impl Drop for HostLog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads one request's first line, writes down its host, refuses it.
fn refuse(mut stream: TcpStream, started: Instant, asked: &Mutex<Vec<Asked>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let mut head = Vec::with_capacity(1024);
    let mut byte = [0u8; 512];
    while !head.windows(2).any(|w| w == b"\r\n") && head.len() < 8 * 1024 {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(n) => head.extend_from_slice(&byte[..n]),
        }
    }
    let line_end = head.windows(2).position(|w| w == b"\r\n").unwrap_or(head.len());
    let line = String::from_utf8_lossy(&head[..line_end]);
    let seen = host_of_request_line(&line);
    let entry = match seen {
        Some((method, host, port)) => Asked { ms: started.elapsed().as_millis(), method, host, port },
        None => Asked { ms: started.elapsed().as_millis(), method: "?".to_string(), host: "(not a proxy request)".to_string(), port: 0 },
    };
    if let Ok(mut list) = asked.lock() {
        list.push(entry);
    }
    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_line_gives_its_host_and_never_its_path() {
        let host = |l: &str| host_of_request_line(l);
        assert_eq!(host("CONNECT edge.example:443 HTTP/1.1"), Some(("CONNECT".into(), "edge.example".into(), 443)));
        assert_eq!(host("CONNECT Edge.Example:8443 HTTP/1.1"), Some(("CONNECT".into(), "edge.example".into(), 8443)));
        assert_eq!(host("GET http://a.example/secret/path?q=1 HTTP/1.1"), Some(("GET".into(), "a.example".into(), 80)));
        assert_eq!(host("POST http://user:pw@a.example:81/x HTTP/1.1"), Some(("POST".into(), "a.example".into(), 81)));
        assert_eq!(host("CONNECT [2001:db8::1]:443 HTTP/1.1"), Some(("CONNECT".into(), "2001:db8::1".into(), 443)));
        assert_eq!(host("GET /local HTTP/1.1"), None);
        assert_eq!(host("hello"), None);
        assert_eq!(host(""), None);
        assert_eq!(host("CONNECT a/../c:443 HTTP/1.1"), None);
        assert_eq!(host("get http://a.example/ HTTP/1.1"), None);
    }

    #[test]
    fn the_process_tree_holds_descendants_only() {
        let all = vec![
            (10, 1, "limo-console.exe".to_string()),
            (11, 10, "msedgewebview2.exe".to_string()),
            (12, 11, "msedgewebview2.exe".to_string()),
            (20, 1, "other.exe".to_string()),
            (21, 20, "msedgewebview2.exe".to_string()),
            (0, 0, "System Idle Process".to_string()),
        ];
        let tree = tree_of(10, &all);
        let pids: Vec<u32> = tree.iter().map(|(p, _)| *p).collect();
        assert_eq!(pids, vec![10, 11, 12]);
        assert_eq!(tree_of(99, &all), vec![]);
    }

    /// The host log on a real loopback port: a CONNECT is written down and
    /// refused, and the port is never the one another program uses.
    #[test]
    fn the_host_log_refuses_and_writes_down_the_host() {
        let log = HostLog::start().unwrap();
        assert_ne!(log.port(), NEVER_PORT);
        let mut client = TcpStream::connect(("127.0.0.1", log.port())).unwrap();
        client.write_all(b"CONNECT config.example:443 HTTP/1.1\r\nHost: config.example:443\r\n\r\n").unwrap();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
        let deadline = Instant::now() + Duration::from_secs(2);
        while log.asked().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let asked = log.asked();
        assert_eq!(asked.len(), 1);
        assert_eq!((asked[0].method.as_str(), asked[0].host.as_str(), asked[0].port), ("CONNECT", "config.example", 443));
    }
}
