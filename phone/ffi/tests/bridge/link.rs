//! The phone's side of the link through this layer, against the testkit's
//! master: an offer read, a pairing that makes the engine without the code
//! secret leaving Rust, and a session that carries the engine's report and
//! the master's grant as bytes.

use std::sync::Arc;

use limo_phone_ffi::testkit::TestFamily;
use limo_phone_ffi::{
    AppEntry, EngineError, GrantOutcome, LinkError, LinkEventKind, Offer, PairRequest, PairedMaster, PhoneEngine,
    PhonePairing, PhoneSession,
};

use crate::common::{GAME, IST, app, at, rules};

/// Carries the phone's bytes to the master and back until the phone has
/// nothing more to say. Returns how many round trips it took.
fn shuttle(family: &TestFamily, mut take: impl FnMut() -> Vec<u8>, mut give: impl FnMut(Vec<u8>), ms: i64, wall: i64) {
    for _ in 0..10 {
        let out = take();
        if out.is_empty() {
            return;
        }
        let back = family.exchange(out, ms, wall).unwrap();
        if !back.is_empty() {
            give(back);
        }
    }
    panic!("the link kept talking");
}

struct Paired {
    _dir: tempfile::TempDir,
    family: Arc<TestFamily>,
    kept_pairing: Vec<u8>,
    engine: Arc<PhoneEngine>,
}

fn pair(start: i64) -> Paired {
    let dir = tempfile::tempdir().unwrap();
    let family = TestFamily::new(dir.path().to_str().unwrap().to_string(), start).unwrap();
    let child = family.add_child("Asha".into(), rules(), start).unwrap();
    let text = family.make_offer(child.clone(), start).unwrap();
    assert!(text.starts_with(b"limo1:"));

    let offer = Offer::from_text(text).unwrap();
    assert_eq!(offer.port(), 47810);
    assert_eq!(offer.addresses(), vec!["192.168.1.20".to_string()]);
    assert_eq!(offer.master_id().len(), 16);
    assert_eq!(offer.expires_at(), start + 600);
    assert!(!offer.looks_old(start));
    assert!(offer.looks_old(start + 601));
    // The secret never shows.
    assert!(format!("{offer:?}").contains("hidden"));

    let request = PairRequest { name: "Asha's phone".into(), platform: "android".into(), android_version: Some(33) };
    let pairing = PhonePairing::start(offer.clone(), request, 1_000).unwrap();
    assert!(!pairing.is_done().unwrap());
    assert_eq!(pairing.paired_master(), Err(LinkError::NotWelcomed));
    assert!(matches!(pairing.make_engine(start), Err(EngineError::NotWelcomed)));
    family.connect(1_000, start).unwrap();
    let mut welcomed = false;
    shuttle(
        &family,
        || pairing.take_outgoing().unwrap(),
        |back| welcomed |= pairing.receive(back, 1_050).unwrap(),
        1_050,
        start,
    );
    assert!(welcomed);
    assert!(pairing.is_done().unwrap());
    assert_eq!(pairing.master_now().unwrap(), Some(start));
    assert_eq!(pairing.tick(1_100).unwrap(), None);

    let kept_pairing = pairing.paired_master().unwrap();
    let engine = pairing.make_engine(start).unwrap();
    // Once only: the welcome and the code secret are gone from the pairing.
    assert!(matches!(pairing.make_engine(start), Err(EngineError::NotWelcomed)));
    assert!(engine.should_save().unwrap());
    let view = engine.view().unwrap().unwrap();
    assert_eq!(view.child_id, child);

    let paired = PairedMaster::from_bytes(kept_pairing.clone()).unwrap();
    assert_eq!(paired.child_id(), child);
    assert_eq!(paired.device_id(), view.device_id);
    assert_eq!(paired.master_id(), offer.master_id());
    assert_eq!(paired.port(), 47810);
    assert_eq!(paired.addresses(), vec!["192.168.1.20".to_string()]);
    Paired { _dir: dir, family, kept_pairing, engine }
}

fn facts_in_reach(wall: i64, elapsed_ms: i64) -> limo_phone_ffi::PhoneFacts {
    limo_phone_ffi::PhoneFacts {
        wall_secs: wall,
        elapsed_ms,
        boot_count: 3,
        utc_offset_secs: IST,
        hours_24: false,
        auto_time: true,
        interactive: true,
        keyguard_locked: false,
        call: limo_phone_ffi::CallState::Idle,
        front: app(GAME, "Game"),
        in_reach: true,
        prodded: false,
        permissions: limo_phone_ffi::PhonePermissions { accessibility: true, usage_access: true },
    }
}

#[test]
fn a_pairing_over_the_link_makes_the_engine_and_a_session_carries_report_and_grant() {
    let start = at(7, 0);
    let Paired { family, kept_pairing, engine, .. } = pair(start);
    let paired = PairedMaster::from_bytes(kept_pairing).unwrap();

    let session = PhoneSession::start(paired, 2_000).unwrap();
    assert!(!session.is_open().unwrap());
    family.connect(2_000, start + 60).unwrap();
    let mut kinds = Vec::new();
    shuttle(
        &family,
        || session.take_outgoing().unwrap(),
        |back| kinds.extend(session.receive(back, 2_010).unwrap().into_iter().map(|e| e.kind)),
        2_010,
        start + 60,
    );
    assert_eq!(kinds, vec![LinkEventKind::Opened, LinkEventKind::Pong]);
    assert!(session.is_open().unwrap());

    // The engine orders a report; the session carries it and the grant.
    let orders = engine.tick(facts_in_reach(start + 60, 60_000)).unwrap();
    assert!(orders.send_report);
    let report = engine.outgoing_report(start + 60).unwrap().unwrap();
    session.send_report(report, 2_020).unwrap();
    assert!(session.report_waiting().unwrap());
    let back = family.exchange(session.take_outgoing().unwrap(), 2_030, start + 60).unwrap();
    let events = session.receive(back, 2_030).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, LinkEventKind::Grant);
    assert_eq!(events[0].master_now, Some(start + 60));
    let grant = events[0].grant.clone().unwrap();
    assert_eq!(engine.take_grant(grant, start + 60).unwrap().outcome, GrantOutcome::Taken);
    assert!(!session.report_waiting().unwrap());
    let orders = engine.tick(facts_in_reach(start + 60, 60_000)).unwrap();
    assert!(!orders.send_report);
    assert!(!orders.time_left.away);

    // Bytes that are not a report are refused, and the session goes on.
    assert_eq!(session.send_report(b"{}".to_vec(), 2_040), Err(LinkError::NotAReport));
    assert!(session.is_open().unwrap());

    // The app list, then goodbye.
    let apps = vec![AppEntry { package_name: GAME.into(), label: "Game".into() }];
    session.send_apps(apps, 2_050).unwrap();
    assert!(family.exchange(session.take_outgoing().unwrap(), 2_050, start + 60).unwrap().is_empty());
    session.send_bye(2_060).unwrap();
    assert!(!session.take_outgoing().unwrap().is_empty());
    assert_eq!(session.tick(2_070).unwrap(), None);
    assert!(matches!(session.send_bye(2_080), Err(LinkError::Closed { .. })));
}

#[test]
fn an_unanswered_report_ends_the_session_saying_so() {
    let start = at(7, 30);
    let Paired { family, kept_pairing, engine, .. } = pair(start);
    let session = PhoneSession::start(PairedMaster::from_bytes(kept_pairing).unwrap(), 5_000).unwrap();
    family.connect(5_000, start).unwrap();
    shuttle(
        &family,
        || session.take_outgoing().unwrap(),
        |back| {
            session.receive(back, 5_000).unwrap();
        },
        5_000,
        start,
    );
    let report = engine.outgoing_report(start).unwrap().unwrap();
    session.send_report(report, 5_100).unwrap();
    // Nothing comes back for ten seconds.
    let ended = session.tick(15_100).unwrap_err();
    assert!(matches!(ended, LinkError::NoAnswerToReport { .. }), "{ended:?}");
    engine.lost_contact().unwrap();
    assert!(matches!(session.tick(15_200), Err(LinkError::Closed { .. })));
}

#[test]
fn a_removed_phone_is_refused_by_its_reason() {
    let start = at(8, 0);
    let Paired { family, kept_pairing, engine, .. } = pair(start);
    let device = engine.view().unwrap().unwrap().device_id;
    family.remove_phone(device, start + 10).unwrap();
    let session = PhoneSession::start(PairedMaster::from_bytes(kept_pairing).unwrap(), 9_000).unwrap();
    family.connect(9_000, start + 20).unwrap();
    let mut refused = None;
    for _ in 0..5 {
        let out = session.take_outgoing().unwrap();
        if out.is_empty() {
            break;
        }
        let back = family.exchange(out, 9_010, start + 20).unwrap();
        if let Err(e) = session.receive(back, 9_010) {
            refused = Some(e);
            break;
        }
    }
    let refused = refused.expect("a refusal");
    assert!(matches!(refused, LinkError::RefusedRemoved { .. }), "{refused:?}");
    assert_eq!(refused.to_string(), "the master PC refused: this phone was removed from the family");
}

#[test]
fn bad_offers_bad_kept_pairings_and_times_below_zero_are_refused_without_quoting_them() {
    for text in [b"hello".to_vec(), b"limo2:AAAA".to_vec(), b"limo1:AAAA".to_vec(), vec![0xff, 0xfe]] {
        let refused = Offer::from_text(text).unwrap_err();
        assert!(matches!(refused, LinkError::BadOffer { .. }), "{refused:?}");
        assert!(!refused.to_string().contains("AAAA"));
    }
    assert!(matches!(PairedMaster::from_bytes(b"{}".to_vec()), Err(LinkError::Malformed { .. })));

    let start = at(9, 0);
    let Paired { kept_pairing, .. } = pair(start);
    // The kept pairing holds the private key; the object never shows it.
    let kept: serde_json::Value = serde_json::from_slice(&kept_pairing).unwrap();
    let private = kept["phone_keys"]["private"].as_str().unwrap().to_string();
    assert_eq!(private.len(), 64);
    let paired = PairedMaster::from_bytes(kept_pairing).unwrap();
    assert!(!format!("{paired:?}").contains(&private));
    assert!(matches!(PhoneSession::start(paired, -1), Err(LinkError::OutOfRange { .. })));
}
