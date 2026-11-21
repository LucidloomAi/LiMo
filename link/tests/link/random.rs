//! Many random inputs, none of which may make either side panic: random
//! bytes at every step, random changes to whole conversations, and random
//! JSON inside good encryption both ways. A seeded generator, so a failure
//! can be run again; `LIMO_LINK_RANDOM=<n>` runs n times as many.

use limo_link::{LinkError, MasterEvent, PhonePairing, PhoneSession, apps_hash};

use crate::common::*;
use crate::raw::{RawHandshake, raw_master};
use crate::wire::*;

fn rounds(base: usize) -> usize {
    let times = std::env::var("LIMO_LINK_RANDOM").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
    base * times.max(1)
}

/// Random bytes: sometimes a length the step allows with random bytes
/// after it, sometimes nothing like a frame at all.
fn noise(rng: &mut Rng, allowed: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    match rng.below(4) {
        0 => {
            let n = rng.below(300);
            out = rng.bytes(n);
        }
        1 => {
            let len = allowed[rng.below(allowed.len())];
            out.extend_from_slice(&len.to_be_bytes());
            out.extend(rng.bytes(len as usize));
        }
        2 => {
            let len = rng.below(200) as u32;
            out.extend_from_slice(&len.to_be_bytes());
            let n = rng.below(250);
            out.extend(rng.bytes(n));
        }
        _ => {
            out.extend_from_slice(b"\x00\x00\x00\x36LiMo");
            out.push(rng.next() as u8);
            out.push(if rng.below(2) == 0 { b'S' } else { b'P' });
            let n = rng.below(80);
            out.extend(rng.bytes(n));
        }
    }
    out
}

/// Feeds bytes in random pieces until something refuses them.
fn feed<T>(rng: &mut Rng, bytes: &[u8], mut into: impl FnMut(&[u8]) -> Result<T, LinkError>) {
    let mut at = 0;
    while at < bytes.len() {
        let n = 1 + rng.below(bytes.len() - at);
        if into(&bytes[at..at + n]).is_err() {
            assert_eq!(into(b"x").err(), Some(LinkError::Closed));
            return;
        }
        at += n;
    }
}

#[test]
fn random_bytes_never_panic_at_any_step() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let offer = family.offer();
    let mut rng = Rng::new(2026);
    let now = family.now();
    for _ in 0..rounds(400) {
        // A master waiting for a first frame.
        let mut link = family.master.accept(now);
        let bytes = noise(&mut rng, &[54, 62]);
        feed(&mut rng, &bytes, |b| family.master.handle(&mut link, b, now).map_err(link_error));
        // A phone waiting for the pairing reply, and for a session's.
        let mut pairing = PhonePairing::start(&offer, request("Phone"), now).unwrap();
        let bytes = noise(&mut rng, &[96]);
        feed(&mut rng, &bytes, |b| pairing.receive(b, now));
        let mut session = PhoneSession::start(&phone.paired, now).unwrap();
        let bytes = noise(&mut rng, &[48]);
        feed(&mut rng, &bytes, |b| session.receive(b, now));
    }
    for _ in 0..rounds(100) {
        // Both sides of an open session.
        let (mut session, mut link) = family.open(&phone);
        let bytes = noise(&mut rng, &[17, 40, 65_535]);
        feed(&mut rng, &bytes, |b| session.receive(b, now));
        let bytes = noise(&mut rng, &[17, 40, 65_535]);
        feed(&mut rng, &bytes, |b| family.master.handle(&mut link, b, now).map_err(link_error));
        // A master half way through a pairing.
        let mut pairing = PhonePairing::start(&offer, request("Phone"), now).unwrap();
        let mut link = family.master.accept(now);
        family.master.handle(&mut link, &pairing.take_outgoing(), now).unwrap();
        pairing.receive(&link.take_outgoing(), now).unwrap();
        let bytes = noise(&mut rng, &[64]);
        feed(&mut rng, &bytes, |b| family.master.handle(&mut link, b, now).map_err(link_error));
    }
}

#[test]
fn random_changes_to_whole_conversations_never_panic_or_bring_anything_false() {
    let honest_pairing = pairing_run(None, 1);
    let honest_session = session_run(None, 1);
    let mut rng = Rng::new(19);
    for i in 0..rounds(60) {
        for (honest, is_pairing) in [(&honest_pairing, true), (&honest_session, false)] {
            let dir = if rng.below(2) == 0 { Dir::ToMaster } else { Dir::ToPhone };
            let index = rng.below(honest.frames[dir as usize] + 1);
            let mutation = match rng.below(11) {
                10 => Mutation::Garble { at: rng.below(70_000), bit: rng.below(8) as u8 },
                k => EVERY_MUTATION[k],
            };
            let plan = Plan { dir, index, mutation };
            let run = if is_pairing { pairing_run(Some(plan), i as u64) } else { session_run(Some(plan), i as u64) };
            assert!(is_prefix(&run.phone_events, &honest.phone_events), "{plan:?}: {run:?}");
            assert!(is_prefix(&run.master_events, &honest.master_events), "{plan:?}: {run:?}");
        }
    }
}

/// Valid messages from a phone, as JSON, to change at random.
fn phone_messages(phone: &mut Phone, now: i64) -> Vec<Vec<u8>> {
    let report = phone.state.report(now, true);
    let apps = vec![limo_link::AppEntry { package: "com.example.game".into(), label: "Game".into() }];
    vec![
        br#"{"say":"ping"}"#.to_vec(),
        br#"{"say":"bye"}"#.to_vec(),
        serde_json::to_vec(&serde_json::json!({"say": "report", "report": report})).unwrap(),
        serde_json::to_vec(&serde_json::json!({"say": "apps", "apps": apps, "hash": apps_hash(&apps)})).unwrap(),
        br#"{"say":"pair","name":"x","form":"child","platform":"android","android_version":33}"#.to_vec(),
        br#"{"say":"about","model":"samsung SM-S911B","android_version":34,"app_version":"0.4.0"}"#.to_vec(),
        br#"{"say":"something_newer","at":1}"#.to_vec(),
        format!("{}{}", "[".repeat(5_000), "]".repeat(5_000)).into_bytes(),
        br#"{"say":"report","report":{"child_id":"","device_id":"","message_number":18446744073709551615}}"#.to_vec(),
        format!(r#"{{"say":"apps","apps":[{}],"hash":""}}"#, vec![r#"{"package":"a","label":"b"}"#; 3_000].join(",")).into_bytes(),
    ]
}

/// Valid messages from a master, as JSON, to change at random.
fn master_messages(grant_json: &serde_json::Value) -> Vec<Vec<u8>> {
    vec![
        br#"{"say":"pong","master_now":1795051800}"#.to_vec(),
        br#"{"say":"report_now"}"#.to_vec(),
        br#"{"say":"refused","why":"removed","speaks":1}"#.to_vec(),
        serde_json::to_vec(&serde_json::json!({"say": "grant", "grant": grant_json, "master_now": 1})).unwrap(),
        serde_json::to_vec(&serde_json::json!({"say": "welcome", "welcome": grant_json, "code_secret": "00", "master_now": 1}))
            .unwrap(),
        br#"{"say":"pong","master_now":-9223372036854775808}"#.to_vec(),
        br#"{"say":"something_newer","at":1}"#.to_vec(),
    ]
}

fn mutate(rng: &mut Rng, mut text: Vec<u8>) -> Vec<u8> {
    for _ in 0..1 + rng.below(3) {
        match rng.below(6) {
            0 if !text.is_empty() => {
                let at = rng.below(text.len());
                text[at] ^= 1 << rng.below(8);
            }
            1 if !text.is_empty() => {
                let at = rng.below(text.len());
                text.truncate(at);
            }
            2 => {
                let at = rng.below(text.len() + 1);
                let n = 1 + rng.below(8);
                let junk = rng.bytes(n);
                text.splice(at..at, junk);
            }
            3 if !text.is_empty() => {
                let at = rng.below(text.len());
                let end = (at + rng.below(20)).min(text.len());
                let copy = text[at..end].to_vec();
                text.splice(at..at, copy);
            }
            4 => {
                let at = rng.below(text.len() + 1);
                let value: &[u8] = [&b"null"[..], b"-1", b"1e999", b"\"\\u0000\"", b"{}", b"[]", b"18446744073709551616"][rng.below(7)];
                text.splice(at..at, value.iter().copied());
            }
            _ => {}
        }
    }
    text
}

#[test]
fn random_json_inside_good_encryption_never_panics_on_either_side() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let mut rng = Rng::new(7);
    let now = family.now();
    let corpus = phone_messages(&mut phone, now.unix);
    let private = *phone.paired.phone_keys.expose_private();
    let master_key = *phone.paired.master_key.as_bytes();
    let mut taken = 0;
    for i in 0..rounds(600) {
        // A hand made phone with the real key says something.
        let (mut raw, first) = RawHandshake::session(&private, &master_key, 1, i as u64);
        let mut link = family.master.accept(now);
        family.master.handle(&mut link, &first, now).unwrap();
        assert_eq!(raw.reply(&link.take_outgoing()).unwrap(), None);
        let mut open = raw.open();
        let pick = rng.below(corpus.len());
        let text = mutate(&mut rng, corpus[pick].clone());
        match family.master.handle(&mut link, &open.seal(&text), now) {
            Ok(events) => {
                taken += 1;
                for event in events {
                    if let MasterEvent::Report { report } = event {
                        assert_eq!(report.device_id, phone.id());
                    }
                }
            }
            Err(e) => {
                let e = link_error(e);
                assert!(!matches!(e, LinkError::Internal { .. }), "{e:?}");
            }
        }
    }
    assert!(taken > 0);

    // And a hand made master with the real key says something to the phone.
    let grant = {
        let (mut session, mut link) = family.open(&phone);
        let report = phone.state.report(now.unix, true);
        session.send_report(&report, now).unwrap();
        pump(&mut session, &mut link, &mut family.master, now).grant().unwrap()
    };
    let grant_json = serde_json::to_value(&grant).unwrap();
    let corpus = master_messages(&grant_json);
    let master_private = *family.master.keys().expose_private();
    let phone_public = *phone.paired.phone_keys.public().as_bytes();
    let mut heard = 0;
    for i in 0..rounds(600) {
        let mut session = PhoneSession::start(&phone.paired, now).unwrap();
        let (mut raw, reply) = raw_master(&master_private, &phone_public, &session.take_outgoing(), i as u64);
        session.receive(&reply, now).unwrap();
        raw.open(&session.take_outgoing()).unwrap();
        // The phone has a report waiting too, so that a grant can come.
        let report = phone.state.report(now.unix, false);
        session.send_report(&report, now).unwrap();
        let pick = rng.below(corpus.len());
        let text = mutate(&mut rng, corpus[pick].clone());
        match session.receive(&raw.seal(&text), now) {
            Ok(_) => heard += 1,
            Err(e) => assert!(!matches!(e, LinkError::Internal { .. }), "{e:?}"),
        }
    }
    assert!(heard > 0);
}
