//! What the link costs, measured on the machine it runs on. Not run by
//! default:
//!
//! ```text
//! cargo test --release --test link cost -- --ignored --nocapture
//! ```

use std::time::Instant;

use limo_link::{DirectoryList, KeyPair, KnownPhone, MasterLink, PhoneSession};

use crate::common::*;

fn per(started: Instant, times: u32) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0 / f64::from(times)
}

#[test]
#[ignore = "a measurement, not a check: run it with --ignored --nocapture"]
fn what_the_link_costs() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    let master_keys = family.master.keys().clone();

    // A whole pairing, both sides, the hub's work included.
    let started = Instant::now();
    for i in 0..20 {
        family.pair(&format!("Phone {i}"));
    }
    println!("a whole pairing, both sides and the hub: {:.2} ms", per(started, 20));

    // A session handshake on the master, with the phone first and last of
    // many it knows: KK names nobody, so each known phone costs a try.
    for known in [1usize, 10, 50] {
        let mut phones: Vec<KnownPhone> = (0..known - 1)
            .map(|i| KnownPhone {
                device_id: format!("device-{i}"),
                child_id: "child".into(),
                key: KeyPair::generate().unwrap().public(),
                removed: false,
            })
            .collect();
        phones.push(KnownPhone {
            device_id: phone.id().into(),
            child_id: family.child.clone(),
            key: phone.paired.phone_keys.public(),
            removed: false,
        });
        let directory = DirectoryList { offers: vec![], phones };
        let started = Instant::now();
        for _ in 0..50 {
            let mut session = PhoneSession::start(&phone.paired, now).unwrap();
            let mut link = MasterLink::accept(&master_keys, now);
            link.receive(&session.take_outgoing(), now, &directory).unwrap();
            session.receive(&link.take_outgoing(), now).unwrap();
        }
        println!("a session handshake, the phone last of {known} known: {:.2} ms", per(started, 50));
    }

    // A long message, near the longest, sealed and opened.
    let (mut session, mut link) = family.open(&phone);
    let apps: Vec<limo_link::AppEntry> = (0..2_000)
        .map(|i| limo_link::AppEntry { package: format!("com.example.app{i:04}.{}", "x".repeat(230)), label: "L".repeat(120) })
        .collect();
    let mut bytes = 0;
    let started = Instant::now();
    for _ in 0..5 {
        session.send_apps(apps.clone(), now).unwrap();
        let frame = session.take_outgoing();
        bytes = frame.len();
        family.master.handle(&mut link, &frame, now).unwrap();
    }
    println!("an app list in a frame of {} KiB, sealed, opened and read: {:.2} ms", bytes / 1024, per(started, 5));
}
