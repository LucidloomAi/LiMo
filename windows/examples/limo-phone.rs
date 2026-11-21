//! `limo-phone`: plays a child's phone over the link, from another PC on the
//! home network, as LiMo for Android will: it pairs with an offer the
//! master made, opens sessions, reports use and takes grants. For trying the
//! master's link by hand; never part of LiMo.
//!
//! ```text
//! limo-phone pair --keep FOLDER [--address IP:PORT]
//!     asks for the pairing offer (paste the "offer" that dev-console's
//!     "request make_offer" printed), pairs, and keeps what a phone keeps in
//!     FOLDER: paired.json (with the phone's private key) and state.json
//! limo-phone use --keep FOLDER [--minutes N] [--app PACKAGE] [--address IP:PORT]
//!     opens a session and, for N minutes (5 when not given), uses the app
//!     (com.example.game when not given) and reports every minute
//! limo-phone look [--secs N]
//!     looks for LiMo's announcement on this network for N seconds (5 when
//!     not given) and prints what it holds
//! ```
//!
//! Delete FOLDER after the run: paired.json holds the phone's private key.

use std::net::{SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use limo_core::DeviceState;
use limo_link::discovery::{SERVICE_TYPE, read_txt};
use limo_link::{Now, Offer, PairedMaster};
use limo_windows::phones::{LinkClock, SystemClock};
use limo_windows::played_phone::PlayedPhone;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent};

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).map(String::as_str)
}

fn pump(clock: &Arc<SystemClock>) -> impl FnMut() -> Now + '_ {
    move || {
        std::thread::sleep(Duration::from_millis(5));
        clock.now()
    }
}

fn keep(folder: &Path, phone: &PlayedPhone) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let paired = phone.paired.to_bytes().map_err(|e| e.to_string())?;
    std::fs::write(folder.join("paired.json"), &paired[..]).map_err(|e| e.to_string())?;
    let state = serde_json::to_vec(&phone.state).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("state.json"), state).map_err(|e| e.to_string())
}

fn load(folder: &Path) -> Result<PlayedPhone, String> {
    let paired = std::fs::read(folder.join("paired.json")).map_err(|e| format!("no paired.json in that folder: {e}"))?;
    let paired = PairedMaster::from_bytes(&paired).map_err(|e| e.to_string())?;
    let state = std::fs::read(folder.join("state.json")).map_err(|e| e.to_string())?;
    let state: DeviceState = serde_json::from_slice(&state).map_err(|e| e.to_string())?;
    Ok(PlayedPhone { paired, state })
}

/// Where to reach the master: the address given, or each of the offer's.
fn addresses(given: Option<&str>, port: u16, known: &[std::net::Ipv4Addr]) -> Result<Vec<SocketAddr>, String> {
    match given {
        Some(text) => Ok(vec![text.parse().map_err(|_| format!("{text} is not an address with a port, such as 192.168.1.20:47810"))?]),
        None => Ok(known.iter().map(|ip| SocketAddr::V4(SocketAddrV4::new(*ip, port))).collect()),
    }
}

fn pair(args: &[String]) -> Result<(), String> {
    let folder = PathBuf::from(value_of(args, "--keep").ok_or("pair needs --keep FOLDER")?);
    println!("Paste the pairing offer (it starts with limo1:) and press Enter:");
    let mut text = String::new();
    std::io::stdin().read_line(&mut text).map_err(|e| e.to_string())?;
    let offer = Offer::from_text(text.trim()).map_err(|e| format!("that is not a pairing offer: {e}"))?;
    let clock = Arc::new(SystemClock::new());
    for address in addresses(value_of(args, "--address"), offer.port(), offer.addresses())? {
        println!("Pairing with the master at {address} ...");
        match PlayedPhone::pair(address, &offer, "A test phone", &mut pump(&clock)) {
            Ok(phone) => {
                keep(&folder, &phone)?;
                println!("Paired: device {} of child {}. Kept in {}.", phone.paired.device_id, phone.paired.child_id, folder.display());
                return Ok(());
            }
            Err(problem) => println!("Not paired at {address}: {problem}"),
        }
    }
    Err("the master could not be paired with at any of its addresses".to_string())
}

fn use_it(args: &[String]) -> Result<(), String> {
    let folder = PathBuf::from(value_of(args, "--keep").ok_or("use needs --keep FOLDER")?);
    let minutes: u32 = value_of(args, "--minutes").map_or(Ok(5), str::parse).map_err(|_| "--minutes takes a whole number")?;
    let app = value_of(args, "--app").unwrap_or("com.example.game").to_string();
    let mut phone = load(&folder)?;
    let clock = Arc::new(SystemClock::new());
    let mut session = None;
    for address in addresses(value_of(args, "--address"), phone.paired.port, &phone.paired.addresses.clone())? {
        match phone.open(address, &mut pump(&clock)) {
            Ok(open) => {
                println!("A session is open with the master at {address}.");
                session = Some(open);
                break;
            }
            Err(problem) => println!("No session at {address}: {problem}"),
        }
    }
    let mut session = session.ok_or("no session could be opened")?;
    let grant = session.exchange(&mut phone, false, &mut pump(&clock)).map_err(|e| e.to_string())?;
    println!("First grant: rules version {}.", grant.rules.version);
    for minute in 1..=minutes {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(60) {
            if session.asked_to_report(Duration::from_secs(1), &mut pump(&clock)).map_err(|e| e.to_string())? {
                session.exchange(&mut phone, false, &mut pump(&clock)).map_err(|e| e.to_string())?;
                println!("The master asked for a report, and had it.");
            }
        }
        let now = clock.now();
        let decision = phone.decide(now.unix, Some(&app));
        if decision.allowed {
            phone.use_app(&app, now.unix, 60);
        }
        session.exchange(&mut phone, false, &mut pump(&clock)).map_err(|e| e.to_string())?;
        let decision = phone.decide(clock.now().unix, Some(&app));
        let left = decision.seconds_left.map_or("no limit".to_string(), |s| format!("{} min {} s", s / 60, s % 60));
        println!("Minute {minute}: {} used; left on the phone: {left}; allowed: {}.", if decision.allowed { &app } else { "nothing" }, decision.allowed);
        keep(&folder, &phone)?;
    }
    session.bye(&mut pump(&clock));
    keep(&folder, &phone)?;
    println!("Goodbye said; the session is closed.");
    Ok(())
}

fn look(args: &[String]) -> Result<(), String> {
    let secs: u64 = value_of(args, "--secs").map_or(Ok(5), str::parse).map_err(|_| "--secs takes a whole number")?;
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let _ = daemon.disable_interface(IfKind::IPv6);
    let found = daemon.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
    println!("Looking for {SERVICE_TYPE} for {secs} seconds ...");
    let give_up = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < give_up {
        if let Ok(ServiceEvent::ServiceResolved(service)) = found.recv_timeout(Duration::from_millis(200)) {
            let txt: Vec<(String, String)> =
                service.get_properties().iter().map(|p| (p.key().to_string(), p.val_str().to_string())).collect();
            let addresses: Vec<String> = service.get_addresses().iter().map(ToString::to_string).collect();
            println!(
                "Found \"{}\" at {} port {}, host {}, TXT {:?}: {}",
                service.get_fullname(),
                addresses.join(", "),
                service.get_port(),
                service.get_hostname(),
                txt,
                match read_txt(&txt) {
                    Ok(announced) => format!("LiMo link version {}, master id {}", announced.version, announced.id.to_hex()),
                    Err(problem) => format!("not readable as LiMo's: {problem:?}"),
                }
            );
        }
    }
    let _ = daemon.shutdown();
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let done = match args.first().map(String::as_str) {
        Some("pair") => pair(&args[1..]),
        Some("use") => use_it(&args[1..]),
        Some("look") => look(&args[1..]),
        _ => Err("limo-phone pair --keep FOLDER | use --keep FOLDER [--minutes N] | look [--secs N]".to_string()),
    };
    if let Err(problem) = done {
        println!("{problem}");
        std::process::exit(1);
    }
}
