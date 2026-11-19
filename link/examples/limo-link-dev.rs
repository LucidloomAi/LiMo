//! `limo-link-dev`: the master played over a temporary family file, and a
//! phone against it, all in memory and with no sockets: a pairing, then a
//! session with a few reports and grants and a goodbye. It prints what
//! crosses, never a secret, and leaves nothing behind.
//!
//! ```text
//! cargo run --example limo-link-dev
//! ```
//!
//! The same calls are what a phone's side makes against the `dev` master
//! in a test (`limo_link::dev::DevMaster`, with the `dev` feature), and what
//! the Windows service and the phone will make around their sockets.

use std::error::Error;
use std::net::Ipv4Addr;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use limo_core::DeviceState;
use limo_link::dev::DevMaster;
use limo_link::{Form, MasterLink, Now, PairRequest, PhoneEvent, PhonePairing, PhoneSession};

struct Clock {
    started: Instant,
    unix_at_start: i64,
    offset_ms: u64,
}

impl Clock {
    fn now(&self) -> Now {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX) + self.offset_ms;
        Now { ms, unix: self.unix_at_start + i64::try_from(ms / 1_000).unwrap_or(0) }
    }

    /// Moves both clocks on, as if time passed.
    fn pass(&mut self, secs: u64) {
        self.offset_ms += secs * 1_000;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let folder = tempfile::tempdir()?;
    let unix = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let mut clock = Clock { started: Instant::now(), unix_at_start: unix, offset_ms: 0 };

    let mut master = DevMaster::open(folder.path(), clock.now().unix)?;
    let child = master.add_child("Asha", 19_800, clock.now().unix)?;
    let offer = master.make_offer(&child, vec![Ipv4Addr::LOCALHOST], clock.now().unix)?;
    println!("master {} made offer {} for Asha, until {}", master.master_id(), offer.id().to_hex(), offer.expires_at());
    println!("the offer's text is {} characters (not shown: it holds the secret)", offer.to_text().len());

    // Pairing.
    let now = clock.now();
    let request = PairRequest { name: "Asha's phone".into(), form: Form::Child, platform: "android".into(), android_version: Some(33) };
    let mut pairing = PhonePairing::start(&offer, request, now)?;
    let mut link: MasterLink = master.accept(now);
    let mut welcomed = None;
    while welcomed.is_none() {
        let up = pairing.take_outgoing();
        println!("phone to master: {} bytes", up.len());
        for event in master.handle(&mut link, &up, now)? {
            println!("  the master heard: {event:?}");
        }
        let down = link.take_outgoing();
        println!("master to phone: {} bytes", down.len());
        welcomed = pairing.receive(&down, now)?;
    }
    let welcomed = welcomed.ok_or("no welcome")?;
    master.handle(&mut link, &pairing.take_outgoing(), now)?;
    println!("paired as {}, with the master's key {:?}", welcomed.paired.device_id, welcomed.paired.master_key);
    let mut state = DeviceState::from_pairing(&welcomed.welcome, now.unix);
    state.set_code_secret(welcomed.code_secret.expose().to_vec())?;

    // A session: a report every two minutes of use, then a goodbye.
    let now = clock.now();
    let mut session = PhoneSession::start(&welcomed.paired, now)?;
    let mut link = master.accept(now);
    carry(&mut session, &mut link, &mut master, &mut state, now)?;
    for _ in 0..3 {
        let start = clock.now().unix;
        clock.pass(120);
        state.record_use("com.example.game", start, clock.now().unix);
        let now = clock.now();
        session.tick(now)?;
        link.tick(now)?;
        let report = state.report(now.unix, true);
        session.send_report(&report, now)?;
        carry(&mut session, &mut link, &mut master, &mut state, now)?;
    }
    let now = clock.now();
    session.send_bye(now)?;
    for event in master.handle(&mut link, &session.take_outgoing(), now)? {
        println!("  the master heard: {event:?}");
    }
    println!("done; the temporary family file goes with its folder");
    Ok(())
}

/// Carries bytes both ways until neither side has anything to say.
fn carry(
    session: &mut PhoneSession,
    link: &mut MasterLink,
    master: &mut DevMaster,
    state: &mut DeviceState,
    now: Now,
) -> Result<(), Box<dyn Error>> {
    loop {
        let up = session.take_outgoing();
        if !up.is_empty() {
            println!("phone to master: {} bytes", up.len());
            for event in master.handle(link, &up, now)? {
                println!("  the master heard: {}", short(&format!("{event:?}")));
            }
        }
        let down = link.take_outgoing();
        if !down.is_empty() {
            println!("master to phone: {} bytes", down.len());
            for event in session.receive(&down, now)? {
                if let PhoneEvent::Grant { grant, .. } = &event {
                    state.take_grant(grant, now.unix);
                    println!("  the phone took grant {}: a lease of {} s today", grant.number, grant.lease_total_secs);
                } else {
                    println!("  the phone heard: {event:?}");
                }
            }
        }
        if up.is_empty() && down.is_empty() {
            return Ok(());
        }
    }
}

fn short(text: &str) -> String {
    if text.chars().count() > 120 { format!("{}...", text.chars().take(120).collect::<String>()) } else { text.to_string() }
}
