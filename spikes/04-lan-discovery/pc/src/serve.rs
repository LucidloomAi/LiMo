//! `serve`: listen for HELLO on TCP and announce the service with mDNS.

use crate::args::{self, ServeArgs};
use crate::clock::{self, StopReason};
use crate::netinfo;
use crate::out::{self, quote};
use crate::protocol::{self, SERVICE_TYPE};
use mdns_sd::{DaemonEvent, IfKind, ServiceDaemon, ServiceInfo, UnregisterStatus};
use std::io::{self, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Connections handled at the same time. More are closed at once.
pub const MAX_OPEN: usize = 8;

pub fn run(a: &ServeArgs) -> u8 {
    let id = protocol::new_token();
    let version = env!("CARGO_PKG_VERSION");
    let bind_ip = if a.loopback {
        Ipv4Addr::LOCALHOST
    } else {
        Ipv4Addr::UNSPECIFIED
    };

    let listener = match TcpListener::bind(SocketAddrV4::new(bind_ip, a.port)) {
        Ok(l) => l,
        Err(e) => {
            out::line(format!(
                "serve failed: could not listen on {bind_ip}:{}: {}",
                a.port,
                bind_words(&e)
            ));
            return 1;
        }
    };
    let port = listener.local_addr().map(|x| x.port()).unwrap_or(a.port);
    let start = Instant::now();
    let deadline = clock::stop_deadline(start, a.seconds);

    out::line(format!(
        "serve start version={version} id={id} port={port} loopback={} stops_in_s={} stops_at={}",
        if a.loopback { "yes" } else { "no" },
        a.seconds,
        clock::local_clock_after(a.seconds)
    ));
    print_listen_addresses(a.loopback, port);

    // Ctrl+C only wakes the main thread; all the stopping is done there.
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    }) {
        out::line(format!(
            "note: could not catch Ctrl+C ({e}); the safety stop still applies"
        ));
    }

    let open = Arc::new(AtomicUsize::new(0));
    {
        let id = id.clone();
        let open = Arc::clone(&open);
        thread::spawn(move || accept_loop(listener, id, version, open));
    }

    let announced = if a.loopback {
        out::line("mdns off (loopback only)");
        None
    } else {
        let name = match &a.name {
            Some(n) => n.clone(),
            None => args::default_name(std::env::var("COMPUTERNAME").ok().as_deref()),
        };
        announce(&name, &id, version, port)
    };

    let mut ctrl_c = false;
    let reason = loop {
        let now = Instant::now();
        if let Some(r) = clock::stop_reason(now, deadline, ctrl_c) {
            break r;
        }
        match stop_rx.recv_timeout(clock::remaining(deadline, now)) {
            Ok(()) => ctrl_c = true,
            Err(RecvTimeoutError::Timeout) => {}
            // Only if the Ctrl+C handler could not be set: wait out the time.
            Err(RecvTimeoutError::Disconnected) => {
                thread::sleep(clock::remaining(deadline, Instant::now()))
            }
        }
    };
    out::line(format!(
        "stop reason={} after_s={}",
        reason.word(),
        start.elapsed().as_secs()
    ));
    if reason == StopReason::SafetyStop {
        out::line(format!("the safety stop ended serve after {} s", a.seconds));
    }

    if let Some(ann) = announced {
        ann.withdraw();
    }
    out::line("serve end");
    0
}

fn bind_words(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::AddrInUse => format!(
            "the port is already in use, perhaps by another serve [{e}]"
        ),
        io::ErrorKind::PermissionDenied => format!(
            "Windows refused the port; it may be in a range Windows keeps for itself (status shows this), so try --port 0 [{e}]"
        ),
        _ => e.to_string(),
    }
}

fn print_listen_addresses(loopback: bool, port: u16) {
    if loopback {
        out::line(format!("listening on 127.0.0.1:{port} (loopback only)"));
        return;
    }
    match netinfo::ipv4_addresses() {
        Ok(list) if !list.is_empty() => {
            for a in list {
                out::line(format!(
                    "listening on {}:{port} interface={} prefix=/{}{}",
                    a.ip,
                    quote(&a.interface),
                    a.prefix,
                    if a.up { "" } else { " (interface down)" }
                ));
            }
        }
        Ok(_) => out::line(format!(
            "listening on 0.0.0.0:{port} (no IPv4 address found on any interface)"
        )),
        Err(e) => out::line(format!(
            "listening on 0.0.0.0:{port} (could not list addresses: {e})"
        )),
    }
}

// TCP

/// Decrements the open connection count when a handler ends, even by panic.
struct OpenGuard(Arc<AtomicUsize>);

impl Drop for OpenGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept_loop(listener: TcpListener, id: String, version: &'static str, open: Arc<AtomicUsize>) {
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let peer = peer_text(&stream);
                if open.fetch_add(1, Ordering::SeqCst) >= MAX_OPEN {
                    open.fetch_sub(1, Ordering::SeqCst);
                    let _ = stream.shutdown(Shutdown::Both);
                    out::line(format!(
                        "connection peer={peer} closed at once: {MAX_OPEN} already open"
                    ));
                    continue;
                }
                let guard = OpenGuard(Arc::clone(&open));
                let id = id.clone();
                let spawned = thread::Builder::new().spawn(move || {
                    let _guard = guard;
                    handle(stream, &peer, &id, version);
                });
                if let Err(e) = spawned {
                    out::line(format!("connection dropped: could not start a thread: {e}"));
                }
            }
            Err(e) => {
                out::line(format!("accept failed: {e}"));
                // Do not spin if accepting keeps failing.
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn peer_text(stream: &TcpStream) -> String {
    match stream.peer_addr() {
        Ok(SocketAddr::V4(a)) => a.to_string(),
        Ok(a) => a.to_string(),
        Err(_) => "unknown".to_string(),
    }
}

/// One connection: read one line (5 s at most), answer, close, log.
fn handle(mut stream: TcpStream, peer: &str, id: &str, version: &str) {
    let deadline = Instant::now() + protocol::IO_LIMIT;
    let read = protocol::read_line_capped(&mut stream, deadline);
    let answer = protocol::answer(&read, id, version, clock::unix_now());
    // The reply is a few bytes. Even after a time out, give it a moment.
    let left = clock::remaining(deadline, Instant::now()).max(Duration::from_millis(500));
    let _ = stream.set_write_timeout(Some(left));
    let sent = match stream.write_all(answer.reply.as_bytes()) {
        Ok(()) => protocol::printable(answer.reply.trim_end_matches('\n').as_bytes()),
        Err(e) => format!("(could not send {}: {e})", protocol::printable(answer.reply.trim_end_matches('\n').as_bytes())),
    };
    let _ = stream.shutdown(Shutdown::Write);
    out::line(format!(
        "connection peer={peer} received={} sent={sent}",
        answer.received
    ));
}

// mDNS

struct Announced {
    daemon: ServiceDaemon,
    fullname: String,
    name: String,
}

fn announce(name: &str, id: &str, version: &str, port: u16) -> Option<Announced> {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            out::line(format!("mdns failed: could not start the responder: {e}"));
            return None;
        }
    };
    // The TCP listener is IPv4 only, so announce IPv4 addresses only.
    if let Err(e) = daemon.disable_interface(IfKind::IPv6) {
        out::line(format!("mdns note: could not turn IPv6 off: {e}"));
    }
    match daemon.monitor() {
        Ok(rx) => {
            thread::spawn(move || {
                while let Ok(ev) = rx.recv() {
                    out::line(event_words(&ev));
                }
            });
        }
        Err(e) => out::line(format!("mdns note: no event monitor: {e}")),
    }

    // A host name of its own, so it never clashes with the name Windows
    // itself answers for (the computer name).
    let host = format!("limo-spike-{id}.local.");
    let props = [("v", version), ("id", id)];
    let info = match ServiceInfo::new(SERVICE_TYPE, name, &host, "", port, &props[..]) {
        Ok(i) => i.enable_addr_auto(),
        Err(e) => {
            out::line(format!("mdns failed: bad service details: {e}"));
            let _ = daemon.shutdown();
            return None;
        }
    };
    let fullname = info.get_fullname().to_string();
    if let Err(e) = daemon.register(info) {
        out::line(format!("mdns failed: could not register: {e}"));
        let _ = daemon.shutdown();
        return None;
    }
    out::line(format!(
        "mdns registered name={} type={SERVICE_TYPE} host={host} port={port} txt=\"v={version} id={id}\"",
        quote(name)
    ));
    print_daemon_state(&daemon);
    Some(Announced {
        daemon,
        fullname,
        name: name.to_string(),
    })
}

/// The responder's sockets and the interfaces where it joined the mDNS
/// multicast group. Shows at once whether, say, the Wi-Fi interface joined.
fn print_daemon_state(daemon: &ServiceDaemon) {
    let rx = match daemon.status() {
        Ok(rx) => rx,
        Err(e) => {
            out::line(format!("mdns state unknown: {e}"));
            return;
        }
    };
    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(mdns_sd::DaemonStatus::Running(state)) => {
            match state.ipv4_socket().bind_error() {
                None => out::line("mdns socket ipv4 udp 5353 bound"),
                Some(e) => out::line(format!("mdns socket ipv4 udp 5353 not bound: {e}")),
            }
            for intf in state.enabled_intfs() {
                if let Some(m) = intf.ipv4() {
                    let addrs: Vec<String> = m.addrs().iter().map(|a| a.to_string()).collect();
                    let joined = match m.join_error() {
                        None => "joined".to_string(),
                        Some(e) => format!("join_failed={}", quote(e)),
                    };
                    out::line(format!(
                        "mdns interface={} index={} ipv4={} {joined}",
                        quote(intf.name()),
                        intf.index(),
                        addrs.join(",")
                    ));
                }
            }
        }
        Ok(other) => out::line(format!("mdns state {other:?}")),
        Err(e) => out::line(format!("mdns state unknown: {e}")),
    }
}

fn event_words(ev: &DaemonEvent) -> String {
    match ev {
        DaemonEvent::Announce(fullname, detail) => format!(
            "mdns announced name={} detail={}",
            quote(&mdns_sd::instance_name_unescaped(fullname)),
            quote(detail)
        ),
        DaemonEvent::Respond(intf) => format!("mdns answered a query via={}", quote(intf)),
        DaemonEvent::IpAdd(ip) => format!("mdns address added ip={ip}"),
        DaemonEvent::IpDel(ip) => format!("mdns address removed ip={ip}"),
        DaemonEvent::NameChange(c) => format!("mdns name conflict, renamed: {c:?}"),
        DaemonEvent::Error(e) => format!("mdns error: {e}"),
        other => format!("mdns event {other:?}"),
    }
}

impl Announced {
    /// Sends the goodbye (records with time to live 0) and stops the responder.
    fn withdraw(self) {
        match self.daemon.unregister(&self.fullname) {
            Ok(rx) => match rx.recv_timeout(Duration::from_secs(3)) {
                Ok(UnregisterStatus::OK) => {
                    out::line(format!("mdns withdrawn name={}", quote(&self.name)))
                }
                Ok(UnregisterStatus::NotFound) => out::line(format!(
                    "mdns withdraw: the responder did not have name={}",
                    quote(&self.name)
                )),
                Err(e) => out::line(format!("mdns withdraw: no answer within 3 s: {e}")),
            },
            Err(e) => out::line(format!("mdns withdraw failed: {e}")),
        }
        // mdns-sd repeats the goodbye once after 120 ms; let it go out.
        thread::sleep(Duration::from_millis(300));
        match self.daemon.shutdown() {
            Ok(rx) => {
                let _ = rx.recv_timeout(Duration::from_secs(2));
            }
            Err(e) => out::line(format!("mdns shutdown: {e}")),
        }
    }
}
