//! A whole pairing and a day of reports and grants, both ends in memory:
//! the phone's own core state on one side, the `dev` master over a real
//! family file on the other, and only the link's bytes between them.

use limo_core::{LockReason, Report};
use limo_link::{LinkError, MasterEvent, MasterLink, PhoneEvent, PhoneSession};

use crate::common::*;

const GAME: &str = "com.example.game";

/// The master's count of the phone's use today, and the phone's own.
fn counted(family: &mut Family, phone: &Phone) -> (u64, u64) {
    let now = family.clock.unix;
    let child = family.child.clone();
    let today = family.master.hub().family_today(now).unwrap();
    let day = today.into_iter().find(|c| c.child_id == child).unwrap();
    let master = day.devices.iter().find(|d| d.device_id == phone.id()).unwrap().used_secs;
    (master, phone.state.holding_at(now).used_secs())
}

/// One minute of the phone: use if it is allowed and wanted, then the
/// session's and the master's clocks, with whatever they send carried
/// across.
fn minute(family: &mut Family, phone: &mut Phone, session: &mut PhoneSession, link: &mut MasterLink, using: bool) {
    let start = family.clock.unix;
    family.clock.advance_secs(MIN);
    let now = family.now();
    if using && phone.state.decide(start, Some(GAME)).allowed {
        phone.state.record_use(GAME, start, now.unix);
    }
    session.tick(now).unwrap();
    link.tick(now).unwrap();
    let pumped = pump(session, link, &mut family.master, now);
    assert_eq!((pumped.phone_error, pumped.master_error), (None, None));
    for event in pumped.phone_events {
        if let PhoneEvent::ReportNow = event {
            family.exchange(phone, session, link, false);
        }
    }
}

#[test]
fn a_whole_pairing_and_a_day_of_reports_and_grants() {
    // 07:00: the phone is paired over the cable, with the offer the PC
    // made for Asha.
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let now = family.clock.unix;
    assert_eq!(phone.state.holding_at(now).away_cap_secs, 60 * 60);

    // 07:00 to 07:30, at home: a game, a report every two minutes asking
    // for more while in use. The master counts every second the phone
    // used, at every exchange.
    let (mut session, mut link) = family.open(&phone);
    let mut exchanges = 0;
    for m in 1..=30 {
        minute(&mut family, &mut phone, &mut session, &mut link, true);
        if m % 2 == 0 {
            let want_more = phone.state.decide(family.clock.unix, Some(GAME)).ask_master;
            family.exchange(&mut phone, &mut session, &mut link, want_more);
            exchanges += 1;
            let (master, own) = counted(&mut family, &phone);
            assert_eq!(master, own, "minute {m}");
        }
    }
    assert_eq!(counted(&mut family, &phone), (30 * 60, 30 * 60));
    assert!(exchanges >= 15);

    // 07:30: it leaves for school with a goodbye.
    let now = family.now();
    session.send_bye(now).unwrap();
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_events, vec![MasterEvent::Bye]);
    assert!(link.is_ended());
    phone.state.lost_contact();

    // 10:00 to 10:45, at school, out of reach: 45 minutes, from its lease
    // and its away allowance. The master knows nothing of it yet.
    family.clock.advance_secs(2 * HOUR + 30 * MIN);
    for _ in 0..45 {
        let start = family.clock.unix;
        family.clock.advance_secs(MIN);
        assert!(phone.state.decide(start, Some(GAME)).allowed);
        phone.state.record_use(GAME, start, family.clock.unix);
    }
    assert_eq!(counted(&mut family, &phone), (30 * 60, 75 * 60));

    // 17:00, home again: a new session, and the school's slices reach the
    // master with the first report.
    let back = family.clock.unix;
    family.clock.advance_secs(START + 10 * HOUR - back);
    let (mut session, mut link) = family.open(&phone);
    family.exchange(&mut phone, &mut session, &mut link, false);
    assert_eq!(counted(&mut family, &phone), (75 * 60, 75 * 60));

    // 17:05: the guardian locks Asha's devices from the PC. The hub wants
    // the phone to hear; the master asks it to report, and the lock
    // arrives with the grant.
    for _ in 0..5 {
        minute(&mut family, &mut phone, &mut session, &mut link, false);
    }
    let now = family.now();
    let child = family.child.clone();
    family.master.hub().lock_now(&child, None, now.unix).unwrap();
    assert!(family.master.prod(&mut link, now).unwrap());
    minute(&mut family, &mut phone, &mut session, &mut link, false);
    let decision = phone.state.decide(family.clock.unix, Some(GAME));
    assert_eq!(decision.device_lock.map(|l| l.reason), Some(LockReason::GuardianLock));

    // Quiet minutes until 17:30: the phone keeps the session alive with a
    // ping every minute it sends nothing else, and the master never finds
    // it silent.
    for _ in 0..23 {
        minute(&mut family, &mut phone, &mut session, &mut link, false);
    }
    // 17:30: lifted again, and the same way back.
    let now = family.now();
    family.master.hub().lift(&child, now.unix).unwrap();
    assert!(family.master.prod(&mut link, now).unwrap());
    minute(&mut family, &mut phone, &mut session, &mut link, false);
    assert!(phone.state.decide(family.clock.unix, Some(GAME)).allowed);

    // 17:30 to 18:30: the evening's game, until today's two hours are
    // used up; the lock comes from the phone's own state.
    for m in 1..=60 {
        minute(&mut family, &mut phone, &mut session, &mut link, true);
        if m % 2 == 0 {
            let want_more = phone.state.decide(family.clock.unix, Some(GAME)).ask_master;
            family.exchange(&mut phone, &mut session, &mut link, want_more);
            let (master, own) = counted(&mut family, &phone);
            assert_eq!(master, own, "evening minute {m}");
        }
    }
    let now = family.clock.unix;
    assert_eq!(counted(&mut family, &phone), (120 * 60, 120 * 60));
    let decision = phone.state.decide(now, Some(GAME));
    assert_eq!(decision.device_lock.map(|l| l.reason), Some(LockReason::TimeUsedUp));

    // 21:00: goodbye for the night.
    let now = family.now();
    session.send_bye(now).unwrap();
    let pumped = pump(&mut session, &mut link, &mut family.master, now);
    assert_eq!(pumped.master_events, vec![MasterEvent::Bye]);
}

#[test]
fn a_session_with_no_answer_leaves_the_phone_spending_only_what_it_holds() {
    let mut family = Family::new();
    let mut phone = family.pair("Asha's phone");
    let (mut session, mut link) = family.open(&phone);
    family.exchange(&mut phone, &mut session, &mut link, true);
    // The PC goes to sleep in the middle of an exchange.
    let now = family.now();
    let report: Report = phone.state.report(now.unix, true);
    session.send_report(&report, now).unwrap();
    let _never_delivered = session.take_outgoing();
    family.clock.advance_ms(limo_link::ANSWER_LIMIT_MS);
    let error = session.tick(family.now()).unwrap_err();
    assert_eq!(error, LinkError::NoAnswer { report_waiting: true });
    phone.state.lost_contact();
    // The phone goes on with what it holds: its lease, then its away time.
    let in_hand = phone.state.holding_at(family.clock.unix).in_hand_secs();
    assert!(in_hand >= 60 * 60);
    // When the PC wakes, a new session counts what was used meanwhile.
    for _ in 0..20 {
        let start = family.clock.unix;
        family.clock.advance_secs(MIN);
        phone.state.record_use(GAME, start, family.clock.unix);
    }
    let (mut session, mut link) = family.open(&phone);
    family.exchange(&mut phone, &mut session, &mut link, false);
    assert_eq!(counted(&mut family, &phone), (20 * 60, 20 * 60));
}
