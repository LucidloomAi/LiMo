//! Sessions after pairing: the KK handshake with pinned keys, reports and
//! grants, prods, apps, goodbyes, the timings, and every way a session is
//! refused.

use limo_link::crypto::{SESSION_HELLO_BYTES, SESSION_REPLY_BYTES};
use limo_link::frame::SESSION_PREAMBLE_BYTES;
use limo_link::{
    ANSWER_LIMIT_MS, AppEntry, HANDSHAKE_LIMIT_MS, KeyPair, LinkError, MasterEvent, PING_EVERY_MS, PhoneEvent,
    PhoneSession, Refusal, SILENCE_LIMIT_MS, apps_hash,
};

use crate::common::*;
use crate::raw::RawHandshake;

#[test]
fn a_session_opens_with_a_ping_and_carries_reports_and_grants() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let now = family.now();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let first = session.take_outgoing();
    assert_eq!(first.len(), 4 + SESSION_PREAMBLE_BYTES + SESSION_HELLO_BYTES);
    assert_eq!(&first[4..10], b"LiMo\x01S");
    // Nothing may be sent before the handshake has finished; that changes
    // nothing.
    let report = phone.state.report(now.unix, true);
    assert_eq!(session.send_report(&report, now), Err(LinkError::NotOpen));
    let mut link = family.master.accept(now);
    assert!(family.master.handle(&mut link, &first, now).unwrap().is_empty());
    // The master has answered, but the phone is not live to it yet.
    assert_eq!(link.phone(), None);
    assert_eq!(link.report_now(), Err(LinkError::NotOpen));
    let reply = link.take_outgoing();
    assert_eq!(reply.len(), 4 + SESSION_REPLY_BYTES);
    assert_eq!(session.receive(&reply, now).unwrap(), vec![PhoneEvent::Opened]);
    // The session's own first message, a ping, proves the phone is live.
    let ping = session.take_outgoing();
    let events = family.master.handle(&mut link, &ping, now).unwrap();
    assert_eq!(events, vec![MasterEvent::Opened { device_id: phone.id().to_string(), child_id: family.child.clone() }]);
    assert_eq!(link.phone().unwrap().device_id, phone.id());
    assert_eq!(session.receive(&link.take_outgoing(), now).unwrap(), vec![PhoneEvent::Pong { master_now: START }]);

    // A report with want_more gets a ten minute lease.
    family.exchange(&mut phone, &mut session, &mut link, true);
    assert_eq!(phone.state.holding_at(now.unix).lease_total_secs, 600);
    assert!(!session.report_waiting());
}

#[test]
fn the_master_asks_for_a_report_when_the_hub_wants_one() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    family.exchange(&mut phone, &mut session, &mut link, false);
    let now = family.now();
    assert!(!family.master.prod(&mut link, now).unwrap());
    // The guardian changes the rules: the hub wants the phone to hear.
    let child = family.child.clone();
    let mut rules = family.master.hub().rules(&child).unwrap();
    rules.lease_minutes = 15;
    family.master.hub().replace_rules(&child, rules, now.unix).unwrap();
    assert!(family.master.prod(&mut link, now).unwrap());
    assert_eq!(session.receive(&link.take_outgoing(), now).unwrap(), vec![PhoneEvent::ReportNow]);
    family.exchange(&mut phone, &mut session, &mut link, false);
    assert_eq!(phone.state.rules().lease_minutes, 15);
}

#[test]
fn the_apps_reach_the_master_with_their_hash() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let apps = vec![
        AppEntry { package: "com.example.game".into(), label: "Game".into() },
        AppEntry { package: "com.example.maps".into(), label: "Maps".into() },
    ];
    let now = family.now();
    session.send_apps(apps.clone(), now).unwrap();
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_events, vec![MasterEvent::Apps { apps: apps.clone(), hash: apps_hash(&apps) }]);
    assert_eq!(family.master.apps_of(phone.id()), Some(&apps[..]));
    // A list out of bounds is refused before it is sent, and changes
    // nothing.
    let too_many = vec![apps[0].clone(); limo_link::messages::MAX_APPS + 1];
    assert!(matches!(session.send_apps(too_many, now), Err(LinkError::OutOfBounds { .. })));
    assert!(session.take_outgoing().is_empty());
    assert!(session.is_open());
}

#[test]
fn bye_ends_the_session_on_both_sides() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let now = family.now();
    session.send_bye(now).unwrap();
    assert!(!session.is_open());
    assert_eq!(family.master.handle(&mut link, &session.take_outgoing(), now).unwrap(), vec![MasterEvent::Bye]);
    assert!(link.is_ended());
    assert_eq!(session.tick(now), Ok(None));
    assert_eq!(link.tick(now), Ok(None));
    assert_eq!(session.send_bye(now), Err(LinkError::Closed));
    assert_eq!(family.master.handle(&mut link, b"more", now).map_err(link_error), Err(LinkError::Closed));
}

#[test]
fn a_phone_with_another_key_is_not_known() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let mut stranger = phone.paired.clone();
    stranger.phone_keys = KeyPair::generate().unwrap();
    let now = family.now();
    let mut session = PhoneSession::start(&stranger, now).unwrap();
    let mut link = family.master.accept(now);
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_error, Some(LinkError::UnknownPhone));
    assert!(pumped.phone_events.is_empty());
    family.clock.advance_ms(HANDSHAKE_LIMIT_MS);
    assert_eq!(session.tick(family.now()), Err(LinkError::HandshakeTooSlow));
}

#[test]
fn a_phone_that_pinned_another_master_gets_nowhere() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let mut misled = phone.paired.clone();
    misled.master_key = KeyPair::generate().unwrap().public();
    let now = family.now();
    let mut session = PhoneSession::start(&misled, now).unwrap();
    let mut link = family.master.accept(now);
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_error, Some(LinkError::UnknownPhone));
    // And a master with another key, that knows the phone's public key,
    // cannot answer it in a way the phone accepts.
    let impostor = KeyPair::generate().unwrap();
    let directory = limo_link::DirectoryList {
        offers: vec![],
        phones: vec![limo_link::KnownPhone {
            device_id: phone.id().to_string(),
            child_id: family.child.clone(),
            key: phone.paired.phone_keys.public(),
            removed: false,
        }],
    };
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let mut fake = limo_link::MasterLink::accept(&impostor, now);
    assert_eq!(fake.receive(&session.take_outgoing(), now, &directory), Err(LinkError::UnknownPhone));
}

#[test]
fn a_removed_phone_is_told_so() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    family.master.remove_phone(&phone.paired.device_id, now.unix).unwrap();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let mut link = family.master.accept(now);
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_error, None);
    assert!(link.is_ended());
    // The reply and the refusal come together; the refusal wins.
    assert!(pumped.phone_events.is_empty());
    assert_eq!(pumped.phone_error, Some(LinkError::Refused(Refusal::Removed)));
    assert!(session.take_outgoing().is_empty());
    // A master that has forgotten the phone altogether cannot even say so.
    family.master.forget_phone(&phone.paired.device_id);
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let mut link = family.master.accept(now);
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_error, Some(LinkError::UnknownPhone));
}

#[test]
fn a_phone_removed_during_its_session_is_refused_at_its_next_report() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let now = family.now();
    family.master.remove_phone(&phone.paired.device_id, now.unix).unwrap();
    let report = phone.state.report(now.unix, false);
    session.send_report(&report, now).unwrap();
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.phone_error, Some(LinkError::Refused(Refusal::Removed)));
    assert!(link.is_ended());
}

#[test]
fn a_report_is_taken_only_for_the_sessions_own_device() {
    let mut family = Family::new();
    let mut asha = family.pair("Asha's phone");
    let mut second = family.pair("Second phone");
    let (mut session, mut link) = family.open(&asha);
    let now = family.now();
    // The phone's own side will not send another device's report.
    let other = second.state.report(now.unix, false);
    assert_eq!(session.send_report(&other, now), Err(LinkError::OtherDevice));
    assert!(session.take_outgoing().is_empty());
    family.exchange(&mut asha, &mut session, &mut link, false);

    // A phone that lies: Asha's keys, the second phone's ids. The master
    // knows the session by its key, and refuses the report.
    let mut liar = asha.paired.clone();
    liar.device_id = second.paired.device_id.clone();
    let mut session = PhoneSession::start(&liar, now).unwrap();
    let mut link = family.master.accept(now);
    let opened = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(
        opened.master_events,
        vec![MasterEvent::Opened { device_id: asha.id().to_string(), child_id: family.child.clone() }]
    );
    let lie = second.state.report(now.unix, true);
    session.send_report(&lie, now).unwrap();
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_error, Some(LinkError::OtherDevice));
    assert_eq!(pumped.phone_error, Some(LinkError::Refused(Refusal::OtherDevice)));
    assert!(pumped.master_events.is_empty());
    // The second phone's day is untouched.
    let child = family.child.clone();
    let today = family.master.hub().family_today(now.unix).unwrap();
    let day = today.iter().find(|c| c.child_id == child).unwrap();
    let device = day.devices.iter().find(|d| d.device_id == second.paired.device_id).unwrap();
    assert_eq!(device.last_heard_at, Some(START));
    assert_eq!(device.holds_secs, Some(60 * 60));
}

#[test]
fn the_master_will_not_send_a_grant_for_another_device_or_one_nobody_asked_for() {
    let mut family = Family::new();
    let mut asha = family.pair("Asha's phone");
    let mut second = family.pair("Second phone");
    let (mut session, mut link) = family.open(&asha);
    let now = family.now();
    let other = second.state.report(now.unix, false);
    let other_grant = family.master.hub().handle_report(&other, now.unix).unwrap();
    // No report is waiting: refused, nothing written.
    assert!(matches!(link.grant(&other_grant, now), Err(LinkError::Unexpected { .. })));
    // A report is waiting, but this grant is another device's.
    let report = asha.state.report(now.unix, false);
    session.send_report(&report, now).unwrap();
    let link_events = link.receive(&session.take_outgoing(), now, &family.master).unwrap();
    assert!(matches!(&link_events[..], [MasterEvent::Report { .. }]));
    assert_eq!(link.grant(&other_grant, now), Err(LinkError::OtherDevice));
    assert!(link.take_outgoing().is_empty());
    // The right grant still goes through: nothing was changed.
    let grant = family.master.hub().handle_report(&report, now.unix).unwrap();
    link.grant(&grant, now).unwrap();
    let got = session.receive(&link.take_outgoing(), now).unwrap();
    assert!(matches!(&got[..], [PhoneEvent::Grant { .. }]));
    // A grant for a session not open, and a welcome on a session.
    assert!(matches!(link.welcome(&grant, &[0; 32], now), Err(LinkError::Unexpected { .. })));
}

#[test]
fn a_report_unanswered_for_ten_seconds_means_the_master_is_out_of_reach() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, _link) = family.open(&phone);
    let now = family.now();
    let report = phone.state.report(now.unix, true);
    session.send_report(&report, now).unwrap();
    // The master never answers (its PC went to sleep).
    let _lost = session.take_outgoing();
    assert!(session.report_waiting());
    assert_eq!(session.tick(now), Ok(Some(now.ms + ANSWER_LIMIT_MS)));
    family.clock.advance_ms(ANSWER_LIMIT_MS - 1);
    assert!(session.tick(family.now()).is_ok());
    family.clock.advance_ms(1);
    assert_eq!(session.tick(family.now()), Err(LinkError::NoAnswer { report_waiting: true }));
    phone.state.lost_contact();
    assert_eq!(session.tick(family.now()), Err(LinkError::Closed));
}

#[test]
fn the_phone_pings_after_a_quiet_minute_and_the_master_drops_a_silent_one() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let opened = family.now();
    assert_eq!(session.tick(opened), Ok(Some(opened.ms + PING_EVERY_MS)));
    assert_eq!(link.tick(opened), Ok(Some(opened.ms + SILENCE_LIMIT_MS)));
    family.clock.advance_ms(PING_EVERY_MS - 1);
    session.tick(family.now()).unwrap();
    assert!(session.take_outgoing().is_empty());
    family.clock.advance_ms(1);
    session.tick(family.now()).unwrap();
    let ping = session.take_outgoing();
    assert!(!ping.is_empty());
    let now = family.now();
    family.master.handle(&mut link, &ping, now).unwrap();
    assert_eq!(session.receive(&link.take_outgoing(), now).unwrap(), vec![PhoneEvent::Pong { master_now: now.unix }]);
    // Three minutes of silence from the phone: the master drops it.
    family.clock.advance_ms(SILENCE_LIMIT_MS - 1);
    assert!(link.tick(family.now()).is_ok());
    family.clock.advance_ms(1);
    assert_eq!(link.tick(family.now()), Err(LinkError::Silent));
    assert!(link.is_ended());
}

#[test]
fn a_handshake_that_does_not_finish_in_five_seconds_ends_on_both_sides() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let start = family.now();
    // A connection that never says anything.
    let mut idle = family.master.accept(start);
    assert_eq!(idle.tick(start), Ok(Some(start.ms + HANDSHAKE_LIMIT_MS)));
    // A phone whose first message reaches the master, but whose ping never
    // does: the master never takes it as live.
    let mut session = PhoneSession::start(&phone.paired, start).unwrap();
    let mut link = family.master.accept(start);
    family.master.handle(&mut link, &session.take_outgoing(), start).unwrap();
    session.receive(&link.take_outgoing(), start).unwrap();
    let _lost_ping = session.take_outgoing();
    family.clock.advance_ms(HANDSHAKE_LIMIT_MS);
    let late = family.now();
    assert_eq!(idle.tick(late), Err(LinkError::HandshakeTooSlow));
    assert_eq!(link.tick(late), Err(LinkError::HandshakeTooSlow));
    assert_eq!(link.phone(), None);
    // A phone whose first message got no reply.
    let mut waiting = PhoneSession::start(&phone.paired, start).unwrap();
    assert_eq!(waiting.tick(late), Err(LinkError::HandshakeTooSlow));
}

#[test]
fn a_replayed_session_start_never_opens_a_session() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let now = family.now();
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    let first = session.take_outgoing();
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    session.receive(&link.take_outgoing(), now).unwrap();
    let ping = session.take_outgoing();
    family.master.handle(&mut link, &ping, now).unwrap();
    session.receive(&link.take_outgoing(), now).unwrap();
    // Someone on the network plays the phone's first frame again, and the
    // ping after it. The master answers the first (it cannot tell yet),
    // but the ping, made with the old keys, does not open the replay.
    let mut replay = family.master.accept(now);
    family.master.handle(&mut replay, &first, now).unwrap();
    assert!(!replay.take_outgoing().is_empty());
    assert_eq!(replay.phone(), None);
    assert_eq!(family.master.handle(&mut replay, &ping, now).map_err(link_error), Err(LinkError::Garbled));
    assert_eq!(replay.phone(), None);
    // The real session goes on.
    family.exchange(&mut phone, &mut session, &mut link, true);
}

#[test]
fn a_phone_of_a_newer_link_is_refused_with_the_version_the_master_speaks() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    let (mut raw, first) = RawHandshake::session(
        phone.paired.phone_keys.expose_private(),
        phone.paired.master_key.as_bytes(),
        2,
        11,
    );
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    assert!(link.is_ended());
    let out = frames(&link.take_outgoing());
    assert_eq!(out.len(), 2);
    assert_eq!(raw.reply(&out[0]).unwrap(), None);
    let mut open = raw.open();
    assert_eq!(open.open(&out[1]).unwrap(), br#"{"say":"refused","why":"version","speaks":1}"#);
}

#[test]
fn a_new_session_takes_over_from_an_old_one_only_once_it_is_live() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut old, mut old_link) = family.open(&phone);
    family.exchange(&mut phone, &mut old, &mut old_link, false);
    // The phone reconnects (its Wi-Fi came back) without a bye.
    let (mut new, mut new_link) = family.open(&phone);
    family.exchange(&mut phone, &mut new, &mut new_link, true);
    // The old link still works as far as the link knows; replacing it is
    // the caller's, on `Opened`.
    assert!(old_link.phone().is_some());
    assert_eq!(new_link.phone().unwrap().device_id, phone.id());
}
