//! Malformed, oversized, truncated and garbled input at every step, and
//! replayed and reordered frames: every frame of a whole pairing and a
//! whole session, changed on the way in every way the wire knows.

use limo_link::frame::MAX_FRAME_BYTES;
use limo_link::{LinkError, PhonePairing, PhoneSession};

use crate::common::*;
use crate::wire::*;

fn plans(frames: [usize; 2]) -> Vec<Plan> {
    let mut plans = Vec::new();
    for (dir, count) in [(Dir::ToMaster, frames[0]), (Dir::ToPhone, frames[1])] {
        for index in 0..count {
            for mutation in EVERY_MUTATION {
                plans.push(Plan { dir, index, mutation });
            }
            // A bit flipped in the length, in the preamble or the start of
            // the body, in the middle, and in the last byte.
            for at in [1usize, 2, 5, 9, 30, 41, 100, 1_000_003] {
                plans.push(Plan { dir, index, mutation: Mutation::Garble { at, bit: (at % 8) as u8 } });
            }
        }
    }
    plans
}

fn check(run: &Run, honest: &Run, plan: Plan, also_harmless: bool) {
    assert!(is_prefix(&run.phone_events, &honest.phone_events), "{plan:?}: the phone heard {:?}", run.phone_events);
    assert!(is_prefix(&run.master_events, &honest.master_events), "{plan:?}: the master heard {:?}", run.master_events);
    // A swap with no frame after it only delays the frame; a stray byte
    // after the last frame a side will ever read is never read.
    let last = plan.index + 1 == honest.frames[plan.dir as usize];
    let harmless = also_harmless
        || (plan.mutation == Mutation::Swap && run.swapped_with_none)
        || (plan.mutation == Mutation::Extra && last);
    if !harmless {
        assert!(run.phone_error.is_some() || run.master_error.is_some(), "{plan:?} went through: {run:?}");
        assert!(!run.finished || run.master_error.is_some() || run.phone_error.is_some(), "{plan:?}");
    }
    let receiver = match plan.dir {
        Dir::ToMaster => &run.master_error,
        Dir::ToPhone => &run.phone_error,
    };
    match plan.mutation {
        // Refused from the length alone.
        Mutation::Huge => assert!(matches!(receiver, Some(LinkError::FrameTooLong { .. })), "{plan:?}: {run:?}"),
        // A bit flipped inside the body of an encrypted message never
        // decrypts.
        Mutation::Garble { at, .. } if at > 40 && at < 1_000_000 => {
            assert!(receiver.is_some(), "{plan:?}: {run:?}");
        }
        _ => {}
    }
}

#[test]
fn an_honest_pairing_and_session_go_through_the_wire() {
    let pairing = pairing_run(None, 1);
    assert!(pairing.finished, "{pairing:?}");
    assert_eq!((pairing.phone_error.clone(), pairing.master_error.clone()), (None, None));
    // The phone: hello, finish, pair, bye. The master: reply, welcome.
    assert_eq!(pairing.frames, [4, 2]);
    assert_eq!(pairing.master_events, vec!["pair", "bye"]);
    let session = session_run(None, 1);
    assert!(session.finished, "{session:?}");
    assert_eq!((session.phone_error.clone(), session.master_error.clone()), (None, None));
    // The phone: hello, ping, report, apps, bye. The master: reply, pong,
    // grant.
    assert_eq!(session.frames, [5, 3]);
    assert_eq!(session.phone_events, vec!["opened", "pong", "grant"]);
    assert_eq!(session.master_events, vec!["opened", "report", "apps", "bye"]);
}

#[test]
fn every_frame_of_a_pairing_changed_on_the_way_ends_it_and_brings_nothing_false() {
    let honest = pairing_run(None, 1);
    let mut tried = 0;
    for (i, plan) in plans(honest.frames).into_iter().enumerate() {
        let run = pairing_run(Some(plan), i as u64);
        // The phone's goodbye after its welcome is a courtesy: lost, cut
        // short or stalled by a wrong length, the master ends the finished
        // pairing quietly after a while; garbled, it ends it at once.
        let bye = plan.dir == Dir::ToMaster && plan.index == 3;
        check(&run, &honest, plan, bye);
        // A pairing never completes for a phone whose handshake or request
        // was changed itself (not played twice, delayed, or followed by a
        // stray byte, when the frame itself still arrived whole first).
        let changed = !matches!(plan.mutation, Mutation::Replay | Mutation::Swap | Mutation::Extra);
        if plan.dir == Dir::ToMaster && plan.index < 3 && changed {
            assert!(!run.phone_events.contains(&"welcome".to_string()), "{plan:?}: {run:?}");
        }
        tried += 1;
    }
    assert_eq!(tried, 6 * 18);
}

#[test]
fn every_frame_of_a_session_changed_on_the_way_ends_it_and_brings_nothing_false() {
    let honest = session_run(None, 1);
    let mut tried = 0;
    for (i, plan) in plans(honest.frames).into_iter().enumerate() {
        let run = session_run(Some(plan), i as u64);
        check(&run, &honest, plan, false);
        // A report or its grant lost on the way: the phone takes it that
        // the master is out of reach.
        let lost = matches!(plan.mutation, Mutation::Drop | Mutation::Cut);
        if lost && ((plan.dir == Dir::ToMaster && plan.index == 2) || (plan.dir == Dir::ToPhone && plan.index == 2)) {
            assert_eq!(run.phone_error, Some(LinkError::NoAnswer { report_waiting: true }), "{plan:?}");
        }
        tried += 1;
    }
    assert_eq!(tried, 8 * 18);
}

#[test]
fn a_length_too_long_is_refused_from_its_four_bytes_alone() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    // Before the handshake: only the two first frame sizes are allowed.
    for (said, expected) in [
        (1_000u32, LinkError::FrameTooLong { said: 1_000, most: 62 }),
        (55, LinkError::WrongSize { step: "first frame", said: 55, expected: 54 }),
        (0, LinkError::WrongSize { step: "first frame", said: 0, expected: 54 }),
    ] {
        let mut link = family.master.accept(now);
        assert_eq!(family.master.handle(&mut link, &said.to_be_bytes(), now).map_err(link_error), Err(expected));
    }
    // A phone waiting for the reply.
    let mut session = PhoneSession::start(&phone.paired, now).unwrap();
    assert_eq!(
        session.receive(&49u32.to_be_bytes(), now),
        Err(LinkError::FrameTooLong { said: 49, most: 48 })
    );
    let offer = family.offer();
    let mut pairing = PhonePairing::start(&offer, request("Phone"), now).unwrap();
    assert_eq!(
        pairing.receive(&95u32.to_be_bytes(), now),
        Err(LinkError::WrongSize { step: "pairing reply", said: 95, expected: 96 })
    );
    // An open session, both ways: one byte over the longest frame.
    let (mut session, mut link) = family.open(&phone);
    let over = u32::try_from(MAX_FRAME_BYTES + 1).unwrap();
    assert_eq!(
        session.receive(&over.to_be_bytes(), now),
        Err(LinkError::FrameTooLong { said: MAX_FRAME_BYTES + 1, most: MAX_FRAME_BYTES })
    );
    assert_eq!(
        family.master.handle(&mut link, &u32::MAX.to_be_bytes(), now).map_err(link_error),
        Err(LinkError::FrameTooLong { said: u32::MAX as usize, most: MAX_FRAME_BYTES })
    );
    // A length no whole pieces have.
    let (mut session, _link) = family.open(&phone);
    assert_eq!(session.receive(&16u32.to_be_bytes(), now), Err(LinkError::BadLength { said: 16 }));
}

#[test]
fn a_frame_that_lies_about_where_its_message_ends_is_refused() {
    // A message cut at a piece boundary, its length changed to match,
    // ends on a piece whose flag says more follows.
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let now = family.now();
    let (mut raw, first) = crate::raw::RawHandshake::session(
        phone.paired.phone_keys.expose_private(),
        phone.paired.master_key.as_bytes(),
        1,
        5,
    );
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    assert_eq!(raw.reply(&link.take_outgoing()).unwrap(), None);
    let mut open = raw.open();
    let more_follows = open.seal_piece(0, br#"{"say":"ping"}"#);
    assert_eq!(family.master.handle(&mut link, &more_follows, now).map_err(link_error), Err(LinkError::Garbled));
    // And a flag that is neither 0 nor 1.
    let (mut raw, first) = crate::raw::RawHandshake::session(
        phone.paired.phone_keys.expose_private(),
        phone.paired.master_key.as_bytes(),
        1,
        6,
    );
    let mut link = family.master.accept(now);
    family.master.handle(&mut link, &first, now).unwrap();
    raw.reply(&link.take_outgoing()).unwrap();
    let mut open = raw.open();
    let odd = open.seal_piece(7, br#"{"say":"ping"}"#);
    assert_eq!(family.master.handle(&mut link, &odd, now).map_err(link_error), Err(LinkError::Garbled));
}

#[test]
fn a_long_message_crosses_in_pieces_and_a_missing_piece_is_noticed() {
    let mut family = Family::new();
    let phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let now = family.now();
    // Two thousand apps with long labels: about 400 KiB, seven pieces.
    let apps: Vec<limo_link::AppEntry> = (0..2_000)
        .map(|i| limo_link::AppEntry { package: format!("com.example.app{i:04}.{}", "x".repeat(100)), label: "L".repeat(80) })
        .collect();
    session.send_apps(apps.clone(), now).unwrap();
    let frame = session.take_outgoing();
    assert!(frame.len() > 6 * 65_535 && frame.len() < 7 * 65_535 + 4, "{}", frame.len());
    family.master.handle(&mut link, &frame, now).unwrap();
    assert_eq!(family.master.apps_of(phone.id()).unwrap(), &apps[..]);
    // The same again, with its last piece taken off and the length made to
    // match: refused.
    session.send_apps(apps, now).unwrap();
    let frame = session.take_outgoing();
    let body_len = frame.len() - 4;
    let cut = body_len - (body_len % 65_535);
    let mut short = u32::try_from(cut).unwrap().to_be_bytes().to_vec();
    short.extend_from_slice(&frame[4..4 + cut]);
    assert_eq!(family.master.handle(&mut link, &short, now).map_err(link_error), Err(LinkError::Garbled));
}

#[test]
fn a_message_too_long_to_send_is_refused_before_anything_is_written() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    let now = family.now();
    // A report with so many slices it is over 1 MiB of JSON.
    let mut report = phone.state.report(now.unix, false);
    let slice = report.slices.first().cloned().unwrap_or(limo_core::Slice {
        id: format!("{}/1", phone.id()),
        child_id: family.child.clone(),
        device_id: phone.id().to_string(),
        app_id: "com.example.game".into(),
        day: 0,
        start: now.unix,
        end: now.unix + 1,
        counts: true,
    });
    report.slices = vec![slice; 8_000];
    assert!(matches!(session.send_report(&report, now), Err(LinkError::MessageTooLong { .. })));
    assert!(session.take_outgoing().is_empty());
    assert!(!session.report_waiting());
    family.exchange(&mut phone, &mut session, &mut link, true);
}
