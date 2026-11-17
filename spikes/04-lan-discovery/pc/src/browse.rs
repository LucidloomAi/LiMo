//! `browse`: look for the service from this PC for N seconds, and send each
//! instance found a HELLO on each of its IPv4 addresses.

use crate::client;
use crate::out::{self, quote};
use crate::protocol::SERVICE_TYPE;
use mdns_sd::{IfKind, RecvTimeoutError, ServiceDaemon, ServiceEvent};
use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

/// Exit code 0 when at least one HELLO was answered properly.
pub fn run(seconds: u64) -> u8 {
    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            out::line(format!("search failed error={}", quote(&e.to_string())));
            return 1;
        }
    };
    // serve announces IPv4 only.
    let _ = daemon.disable_interface(IfKind::IPv6);
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let rx = match daemon.browse(SERVICE_TYPE) {
        Ok(rx) => rx,
        Err(e) => {
            out::line(format!("search failed error={}", quote(&e.to_string())));
            let _ = daemon.shutdown();
            return 1;
        }
    };
    out::line(format!("search started type={SERVICE_TYPE} for_s={seconds}"));

    let mut found: BTreeSet<String> = BTreeSet::new();
    // Per instance, the IPv4 addresses already greeted.
    let mut greeted: HashMap<String, BTreeSet<IpAddr>> = HashMap::new();
    let (mut ok, mut failed) = (0u32, 0u32);

    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let ev = match rx.recv_timeout(left) {
            Ok(ev) => ev,
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => {
                out::line("search failed error=\"the mDNS responder stopped\"");
                break;
            }
        };
        let after_ms = start.elapsed().as_millis();
        match ev {
            ServiceEvent::SearchStarted(_) => {}
            ServiceEvent::ServiceFound(_, fullname) => {
                let name = mdns_sd::instance_name_unescaped(&fullname);
                if found.insert(fullname) {
                    out::line(format!("found name={} after_ms={after_ms}", quote(&name)));
                }
            }
            ServiceEvent::ServiceResolved(info) => {
                let name = mdns_sd::instance_name_unescaped(&info.fullname);
                found.insert(info.fullname.clone());
                let mut addrs: Vec<IpAddr> = info.addresses.iter().map(|a| a.to_ip_addr()).collect();
                addrs.sort();
                let txt: Vec<String> = info
                    .txt_properties
                    .iter()
                    .map(|p| format!("{}={}", p.key(), p.val_str()))
                    .collect();
                let id = info
                    .txt_properties
                    .get("id")
                    .map(|p| p.val_str().to_string())
                    .unwrap_or_else(|| "?".to_string());
                out::line(format!(
                    "resolved name={} host={} addresses={} port={} id={id} txt={} after_ms={after_ms}",
                    quote(&name),
                    info.host,
                    join(&addrs),
                    info.port,
                    quote(&txt.join(" "))
                ));
                let done = greeted.entry(info.fullname.clone()).or_default();
                for ip in addrs.iter().filter(|ip| ip.is_ipv4()) {
                    if done.insert(*ip) {
                        if client::say_hello(SocketAddr::new(*ip, info.port), "") {
                            ok += 1;
                        } else {
                            failed += 1;
                        }
                    }
                }
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                let name = mdns_sd::instance_name_unescaped(&fullname);
                out::line(format!("removed name={} after_ms={after_ms}", quote(&name)));
            }
            ServiceEvent::SearchStopped(_) => {}
            other => out::line(format!("mdns event {other:?}")),
        }
    }

    let _ = daemon.stop_browse(SERVICE_TYPE);
    if let Ok(rx) = daemon.shutdown() {
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    out::line(format!(
        "search done found={} hello_ok={ok} hello_failed={failed} after_s={seconds}",
        found.len()
    ));
    if ok > 0 {
        0
    } else {
        1
    }
}

fn join(addrs: &[IpAddr]) -> String {
    if addrs.is_empty() {
        return "none".to_string();
    }
    addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",")
}
