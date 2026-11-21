//! The link with the family's phones, over real loopback sockets: a phone
//! played with the link's own phone side and the core's device, against
//! the real service, its real listener and a real family file. Pairing with
//! an offer from the door, sessions through the cable, reports and grants, a
//! whole day of one budget shared by the PC and a phone, prods, a removed
//! phone told so, the announcement; and hostile input on the socket, none
//! of which slows the PC's counting.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use limo_agent::{LockWhy, SessionState, Show};
use limo_core::{DailyBudget, WeekBudgets};
use limo_link::{AppEntry, KeyPair, Offer, PairedMaster, Refusal, apps_hash};
use limo_windows::console::{ProblemKind, Reply, Request};
use limo_windows::os::DoorId;
use limo_windows::phones::{MAX_CALLS_PER_MINUTE, MAX_LINK_PER_ADDRESS, Way};
use limo_windows::played_phone::{PhoneProblem, PlayedPhone, Wire};
use limo_windows::protocol::Secret;

use crate::common::*;

const GUARDIAN: &str = "S-1-5-21-1000";
const ASHA: &str = "S-1-5-21-1001";
const GAME: &str = "com.example.game";

/// A PC with Asha and `minutes` a day, the PIN, the link on loopback, and a
/// console signed in from the guardian's account.
fn family_with_the_link(minutes: u32) -> (Pc, String, SocketAddr, DoorId) {
    let mut pc = Pc::new(at(7, 0));
    let asha = pc.add_child("Asha", ASHA, minutes);
    let now = pc.now();
    pc.hub().set_pin(PIN, now).unwrap();
    pc.os().accounts = vec![account(GUARDIAN, "guardian", true), account(ASHA, "asha", false)];
    pc.login(1, GUARDIAN, "guardian");
    pc.os().sessions[0].is_administrator = true;
    let address = pc.with_link();
    pc.run(1);
    let door = pc.console(GUARDIAN, 1);
    pc.hello(door);
    pc.sign_in(door);
    (pc, asha, address, door)
}

/// An offer for the child, made at the door, as the console asks for it.
fn offer_for(pc: &mut Pc, door: DoorId, child: &str) -> Offer {
    let made = pc.done(door, Request::MakeOffer { child: child.into() });
    Offer::from_text(made["offer"].as_str().unwrap()).unwrap()
}

/// A phone paired with Asha over the cable.
fn paired_phone(pc: &mut Pc, door: DoorId, asha: &str, address: SocketAddr) -> PlayedPhone {
    let offer = offer_for(pc, door, asha);
    PlayedPhone::pair(address, &offer, "Asha's phone", &mut || pc.answer_link()).unwrap()
}

/// Moves both clocks on by `secs` and steps once.
fn leap(pc: &mut Pc, secs: u64) {
    let os = pc.os();
    os.steady_ms += secs * 1_000;
    os.wall_ms += (secs * 1_000) as i64;
    pc.step();
}

/// Lets the phone's session read what the master sent, for a few
/// milliseconds of real time: a story leaps the fake clock, which a real
/// phone's never does, so an answer in flight must land first.
fn settle(pc: &mut Pc, session: &mut limo_windows::played_phone::Session) {
    for _ in 0..15 {
        session.idle(&mut || pc.answer_link()).unwrap();
        std::thread::sleep(Duration::from_millis(3));
    }
}

/// Steps until `done` says so, or ten seconds of real time pass.
fn until(pc: &mut Pc, mut done: impl FnMut(&mut Pc) -> bool) -> bool {
    let give_up = Instant::now() + Duration::from_secs(10);
    while Instant::now() < give_up {
        pc.step();
        if done(pc) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn a_phone_pairs_with_an_offer_from_the_door_and_its_session_comes_through_the_cable() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    assert!(pc.announced.lock().unwrap().is_empty(), "nothing announced while the family has no phone");
    let made = pc.done(door, Request::MakeOffer { child: asha.clone() });
    assert_eq!(made["port"].as_u64(), Some(u64::from(address.port())));
    assert_eq!(made["addresses"][0], "192.168.1.20");
    let text = made["offer"].as_str().unwrap().to_string();
    assert!(text.starts_with("limo1:"));
    let offer = Offer::from_text(&text).unwrap();
    let mut phone = PlayedPhone::pair(address, &offer, "Asha's phone", &mut || pc.answer_link()).unwrap();
    assert_eq!(phone.paired.child_id, asha);
    // The family's file keeps it, with its key.
    let phones = pc.hub().phones(&asha).unwrap();
    assert_eq!(phones.len(), 1);
    assert!(phones[0].has_link_key && phones[0].name == "Asha's phone");
    // Its session, through the cable: a report and its grant.
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    let grant = session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    assert_eq!(grant.device_id, phone.paired.device_id);
    let sessions = pc.service.phone_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!((sessions[0].way, sessions[0].device_id.clone()), (Way::Cable, phone.paired.device_id.clone()));
    let link = pc.done(door, Request::DeviceLink { device: phone.paired.device_id.clone() });
    assert_eq!(link["sessions"][0]["way"], "cable", "{link}");
    // With a phone paired, the master announces itself; the log says what.
    assert!(until(&mut pc, |pc| !pc.announced.lock().unwrap().is_empty()));
    let said = pc.announced.lock().unwrap().clone();
    assert_eq!(said, vec![format!("announce {} {}", offer.master_id().to_hex(), address.port())]);
    assert_eq!(pc.logged("LiMo announces itself on the home network: service _limo._tcp.local.").len(), 1);
    // The offer's secret never reaches the log.
    assert!(pc.logged(&text).is_empty() && pc.logged(&text[6..30]).is_empty());
    assert_eq!(pc.logged("A phone was paired with a child through the cable").len(), 1);
    // Its apps are kept.
    let apps = vec![AppEntry { package: GAME.into(), label: "Game".into() }];
    let hash = apps_hash(&apps);
    session.send_apps(apps, &mut || pc.answer_link()).unwrap();
    let device = phone.paired.device_id.clone();
    assert!(until(&mut pc, |pc| pc.hub().phone_apps(&device).unwrap().is_some()));
    assert_eq!(pc.hub().phone_apps(&device).unwrap().unwrap().hash, hash);
    session.bye(&mut || pc.answer_link());
    assert!(until(&mut pc, |pc| pc.service.phone_sessions().is_empty()));
}

#[test]
fn an_offer_works_once_and_only_while_it_lasts() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    let offer = offer_for(&mut pc, door, &asha);
    PlayedPhone::pair(address, &offer, "Asha's phone", &mut || pc.answer_link()).unwrap();
    let again = PlayedPhone::pair(address, &offer, "Another phone", &mut || pc.answer_link()).unwrap_err();
    assert_eq!(again, PhoneProblem::Refused(Refusal::OfferUsed));
    let late = offer_for(&mut pc, door, &asha);
    leap(&mut pc, 11 * 60);
    let expired = PlayedPhone::pair(address, &late, "A late phone", &mut || pc.answer_link()).unwrap_err();
    assert_eq!(expired, PhoneProblem::Refused(Refusal::OfferExpired));
    assert_eq!(pc.hub().phones(&asha).unwrap().len(), 1);
    // An offer for a child the family does not have is not made.
    let none = pc.ask(door, Request::MakeOffer { child: "no such child".into() });
    assert!(matches!(none, Some(Reply::Problem { problem }) if problem.kind == ProblemKind::NotFound));
}

#[test]
fn a_removed_phone_is_told_so_at_its_next_contact_and_the_family_notes_it() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    let phone = paired_phone(&mut pc, door, &asha, address);
    let device = phone.paired.device_id.clone();
    pc.run(1);
    pc.done(door, Request::RemoveDevice { device: device.clone(), password: Secret::new(PASSWORD.into()) });
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    let mut told = None;
    let give_up = Instant::now() + Duration::from_secs(10);
    while told.is_none() && Instant::now() < give_up {
        if let Err(problem) = session.idle(&mut || pc.answer_link()) {
            told = Some(problem);
        }
    }
    assert_eq!(told, Some(PhoneProblem::Refused(Refusal::Removed)));
    let released = pc.hub().phone(&device).unwrap().released_at;
    assert!(released.is_some(), "the family's file notes that it was told");
    assert_eq!(pc.logged("connected, and was told so").len(), 1);
    // With no phone left, the announcement is withdrawn.
    assert!(until(&mut pc, |pc| pc.announced.lock().unwrap().last().is_some_and(|l| l == "withdraw")));
}

#[test]
fn a_newer_session_of_a_phone_takes_the_place_of_the_older() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    let phone = paired_phone(&mut pc, door, &asha, address);
    let mut first = phone.open(address, &mut || pc.answer_link()).unwrap();
    let _second = phone.open(address, &mut || pc.answer_link()).unwrap();
    assert!(first.wire().closed(&mut || pc.answer_link()), "the older session is closed");
    assert!(until(&mut pc, |pc| pc.service.phone_sessions().len() == 1));
}

#[test]
fn a_lock_from_the_console_reaches_the_phone_within_seconds() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    let mut phone = paired_phone(&mut pc, door, &asha, address);
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    let now = pc.now();
    assert!(phone.decide(now, Some(GAME)).allowed);
    pc.done(door, Request::LockNow { child: asha.clone(), until: None });
    // The books want to hear from the phone: it is asked to report.
    let asked = session.asked_to_report(Duration::from_secs(10), &mut || {
        pc.step();
        pc.link_now()
    });
    assert!(asked.unwrap(), "the phone was asked to report");
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    let now = pc.now();
    let decision = phone.decide(now, Some(GAME));
    assert!(!decision.allowed, "{decision:?}");
}

/// The day of the link's own stories, on the PC: Asha has 30 minutes. In
/// the morning her phone uses 10, through the cable; in the afternoon the
/// PC uses 10; in the evening the phone has the last 10 and uses them; then
/// the PC is locked, the time used up on the phone. The master's count is
/// the sum at every step.
#[test]
fn a_whole_day_of_one_budget_shared_by_the_pc_and_a_phone() {
    let (mut pc, asha, address, door) = family_with_the_link(30);
    let mut phone = paired_phone(&mut pc, door, &asha, address);
    let device = phone.paired.device_id.clone();

    // 07:00 to 07:10, the phone: use every minute, a report every two.
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    for minute in 1..=10 {
        leap(&mut pc, 60);
        let now = pc.now();
        phone.use_app(GAME, now, 60);
        if minute % 2 == 0 {
            session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
        }
        settle(&mut pc, &mut session);
    }
    session.bye(&mut || pc.answer_link());
    let now = pc.now();
    let used = pc.hub().child_today(&asha, now).unwrap().used_secs;
    assert!((9 * 60..=10 * 60).contains(&used), "the morning on the phone: {used}");

    // 16:00, the PC, ten minutes.
    let os = pc.os();
    os.steady_ms += 9 * 3_600 * 1_000;
    os.wall_ms += 9 * 3_600 * 1_000;
    pc.login(2, ASHA, "asha");
    pc.run(1);
    pc.greet(2);
    for _ in 0..600 {
        pc.report(2, 0, None, false);
        pc.run(1);
    }
    pc.set_state(2, SessionState::Disconnected);
    pc.run(2);
    let now = pc.now();
    let used = pc.hub().child_today(&asha, now).unwrap().used_secs;
    assert!((19 * 60..=20 * 60 + 5).contains(&used), "the morning and the afternoon: {used}");

    // 18:00, the phone again: it hears what is left, the PC's minutes
    // taken off. The PC still holds back what is left of its own lease
    // (at most half of ten minutes) until it reports it unused: the core's
    // guarantee that two devices never spend the same minute.
    leap(&mut pc, 2 * 3_600 - 600);
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    let grant = session.exchange(&mut phone, true, &mut || pc.answer_link()).unwrap();
    let others = grant.others_used_secs.unwrap_or(0);
    assert!(others >= 10 * 60 - 5, "the PC's use is in the grant: {others}");
    let now = pc.now();
    let left = phone.decide(now, Some(GAME)).seconds_left.unwrap_or(0);
    assert!((5 * 60..=10 * 60 + 5).contains(&left), "what is left on the phone: {left}");
    let mut phone_minutes = 0;
    loop {
        leap(&mut pc, 60);
        let now = pc.now();
        if !phone.decide(now, Some(GAME)).allowed || phone_minutes > 15 {
            break;
        }
        phone.use_app(GAME, now, 60);
        session.exchange(&mut phone, true, &mut || pc.answer_link()).unwrap();
        settle(&mut pc, &mut session);
        phone_minutes += 1;
    }
    assert!((4..=11).contains(&phone_minutes), "the phone used what it was given: {phone_minutes}");
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    session.bye(&mut || pc.answer_link());

    // 18:20, the PC: Asha uses what it held, and then it locks, the day's
    // time used up on both.
    pc.set_state(2, SessionState::InUse);
    let mut pc_secs = 0;
    while pc_secs < 12 * 60 {
        pc.report(2, 0, None, false);
        pc.run(1);
        pc_secs += 1;
        if matches!(pc.last_show(2).map(|s| s.2), Some(Show::Locked { .. })) {
            break;
        }
    }
    match pc.last_show(2).unwrap().2 {
        Show::Locked { screen } => assert_eq!(screen.why, LockWhy::TimeUsedUp),
        other => panic!("{other:?}"),
    }
    let evening = phone_minutes * 60 + pc_secs;
    assert!((9 * 60..=11 * 60 + 5).contains(&evening), "the evening's ten minutes, on both: {evening}");

    // The master's count is one budget: 30 minutes in all, the phone's and
    // the PC's together, and nothing left.
    let now = pc.now();
    let today = pc.hub().child_today(&asha, now).unwrap();
    assert!((29 * 60..=31 * 60).contains(&today.used_secs), "{today:?}");
    assert_eq!(today.left_secs, Some(0));
    let phone_today = today.devices.iter().find(|d| d.device_id == device).unwrap();
    assert!(phone_today.used_secs >= 13 * 60, "{phone_today:?}");
}

// --------------------------------------------------------------------------
// Hostile input on the socket. None of it may slow the PC's counting.
// --------------------------------------------------------------------------

/// A PC with Asha counting in session 2, and the link on loopback.
fn counting_with_the_link() -> (Pc, String, SocketAddr, DoorId) {
    let (mut pc, asha, address, door) = family_with_the_link(120);
    pc.login(2, ASHA, "asha");
    pc.run(1);
    pc.greet(2);
    pc.run(2);
    (pc, asha, address, door)
}

/// Runs `secs` of the fake PC with the helper reporting, and says how many
/// seconds were counted for Asha meanwhile.
fn counted_over(pc: &mut Pc, asha: &str, secs: u64) -> u64 {
    let before = pc.counted(asha);
    for _ in 0..secs {
        pc.report(2, 0, None, false);
        pc.run(1);
    }
    pc.counted(asha) - before
}

#[test]
fn garbage_before_the_handshake_is_closed_and_reaches_nothing() {
    let (mut pc, asha, address, _door) = counting_with_the_link();
    for garbage in [b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(), vec![0u8; 4], vec![0xff; 70], b"LiMo".to_vec()] {
        let mut wire = Wire::connect(address).unwrap();
        wire.send(&garbage).unwrap();
        let mut pump = || {
            pc.step();
            pc.link_now()
        };
        assert!(wire.closed(&mut pump), "{garbage:?}");
    }
    assert!(counted_over(&mut pc, &asha, 10) >= 9);
    assert!(!pc.logged("A connection to the link from 127.0.0.1 was closed").is_empty());
    assert!(pc.hub().phones(&asha).unwrap().is_empty());
}

#[test]
fn a_handshake_with_a_key_this_pc_never_paired_is_closed() {
    let (mut pc, asha, address, door) = counting_with_the_link();
    let phone = paired_phone(&mut pc, door, &asha, address);
    // The same device's id, the right master, but a key of its own.
    let stranger = PlayedPhone {
        paired: PairedMaster { phone_keys: KeyPair::generate().unwrap(), ..phone.paired.clone() },
        state: phone.state.clone(),
    };
    let opened = stranger.open(address, &mut || {
        pc.step();
        pc.link_now()
    });
    assert!(opened.is_err(), "no session opens with a key the PC never paired");
    assert!(until(&mut pc, |pc| !pc.logged("a phone this PC does not know tried to connect").is_empty()));
    assert!(counted_over(&mut pc, &asha, 5) >= 4);
}

#[test]
fn a_peer_that_connects_and_says_nothing_is_closed_after_five_seconds() {
    let (mut pc, asha, address, _door) = counting_with_the_link();
    let mut wire = Wire::connect(address).unwrap();
    assert!(counted_over(&mut pc, &asha, 3) >= 2);
    assert!(wire.take().is_ok(), "still open after three seconds");
    let counted = counted_over(&mut pc, &asha, 4);
    assert!(counted >= 3);
    let mut pump = || {
        pc.step();
        pc.link_now()
    };
    assert!(wire.closed(&mut pump));
    assert!(until(&mut pc, |pc| !pc.logged("the handshake did not finish in time").is_empty()));
}

#[test]
fn a_flood_of_connections_or_of_reports_is_cut_off_and_the_pc_counts_on() {
    let (mut pc, asha, address, door) = counting_with_the_link();
    // More connections from one address than it may have: the rest are
    // closed at once.
    let mut held: Vec<Wire> = (0..MAX_LINK_PER_ADDRESS).map(|_| Wire::connect(address).unwrap()).collect();
    std::thread::sleep(Duration::from_millis(100));
    let mut extra = Wire::connect(address).unwrap();
    let mut pump = || pc.link_now();
    assert!(extra.closed(&mut pump), "a connection past the most from one address is closed at once");
    held.clear();
    std::thread::sleep(Duration::from_millis(100));
    // A frame said to be four gigabytes long.
    let mut wire = Wire::connect(address).unwrap();
    wire.send(&[0xff, 0xff, 0xff, 0xff]).unwrap();
    let mut pump = || {
        pc.step();
        pc.link_now()
    };
    assert!(wire.closed(&mut pump));
    // A paired phone that reports far more often than a phone does.
    let mut phone = paired_phone(&mut pc, door, &asha, address);
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    let mut answered = 0;
    for _ in 0..(MAX_CALLS_PER_MINUTE + 10) {
        match session.exchange(&mut phone, false, &mut || pc.answer_link()) {
            Ok(_) => answered += 1,
            Err(_) => break,
        }
    }
    assert_eq!(answered, MAX_CALLS_PER_MINUTE, "cut off after {MAX_CALLS_PER_MINUTE} calls in a minute");
    assert!(until(&mut pc, |pc| !pc.logged(&format!("it asked more than {MAX_CALLS_PER_MINUTE} times in a minute")).is_empty()));
    assert!(counted_over(&mut pc, &asha, 10) >= 9);
}

#[test]
fn an_offer_needs_the_link_running() {
    let mut pc = Pc::new(at(7, 0));
    let asha = pc.add_child("Asha", ASHA, 60);
    pc.os().accounts = vec![account(GUARDIAN, "guardian", true)];
    let door = pc.console(GUARDIAN, 1);
    pc.hello(door);
    pc.sign_in(door);
    match pc.ask(door, Request::MakeOffer { child: asha }) {
        Some(Reply::Problem { problem }) => {
            assert_eq!(problem.kind, ProblemKind::Failed);
            assert!(problem.message.contains("link for phones is not running"), "{}", problem.message);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_rules_changed_at_the_console_reach_the_phone_in_its_next_grant() {
    let (mut pc, asha, address, door) = family_with_the_link(60);
    let mut phone = paired_phone(&mut pc, door, &asha, address);
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    let mut rules: limo_core::RuleSet = serde_json::from_value(pc.done(door, Request::Rules { child: asha.clone() })).unwrap();
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(5));
    rules.blocked_apps = vec![GAME.into()];
    pc.done(door, Request::ReplaceRules { child: asha.clone(), rules });
    session.exchange(&mut phone, false, &mut || pc.answer_link()).unwrap();
    assert!(phone.state.rules().blocked_apps.contains(&GAME.to_string()));
    let now = pc.now();
    assert!(!phone.decide(now, Some(GAME)).allowed);
}

/// The service's step and the link's turn between two steps, with a phone
/// reporting every second through the cable and garbage connections coming
/// meanwhile, measured with the real clock and printed (`cargo test --
/// --nocapture` shows it).
#[test]
fn the_step_time_with_a_phone_talking() {
    let (mut pc, asha, address, door) = counting_with_the_link();
    let mut phone = paired_phone(&mut pc, door, &asha, address);
    let mut session = phone.open(address, &mut || pc.answer_link()).unwrap();
    let mut measure = |pc: &mut Pc, talking: bool| -> (Vec<Duration>, Vec<Duration>, u64) {
        let before = pc.counted(&asha);
        let (mut steps, mut turns) = (Vec::new(), Vec::new());
        let mut grants = 0;
        for n in 0..300u32 {
            pc.report(2, 0, None, false);
            if talking && n % 10 == 0 {
                // Sent, not waited for: the measured turns answer it.
                let now = pc.now();
                phone.use_app(GAME, now, 1);
                session.send_report(&mut phone, false, &mut || pc.link_now()).unwrap();
                if n % 50 == 0 {
                    let mut wire = Wire::connect(address).unwrap();
                    let _ = wire.send(b"not a link at all");
                }
            }
            // A moment of real time for the connection's thread.
            std::thread::sleep(Duration::from_millis(2));
            let os = pc.os();
            os.steady_ms += 100;
            os.wall_ms += 100;
            let (ms, unix) = (os.steady_ms, os.wall_ms.div_euclid(1_000));
            pc.clock.set(ms, unix);
            let started = Instant::now();
            pc.service.step();
            steps.push(started.elapsed());
            let started = Instant::now();
            pc.service.door();
            pc.service.link();
            turns.push(started.elapsed());
            if talking {
                session.idle(&mut || pc.link_now()).unwrap();
                let now = pc.link_now();
                grants += session.take_held_grants(&mut phone, now);
            }
        }
        if talking {
            assert!(grants >= 28, "the reports were answered in the measured turns: {grants}");
        }
        (steps, turns, pc.counted(&asha) - before)
    };
    let (quiet_steps, _, quiet_counted) = measure(&mut pc, false);
    let (steps, turns, counted) = measure(&mut pc, true);
    let ms = |d: &Duration| d.as_secs_f64() * 1_000.0;
    let mean = |v: &[Duration]| v.iter().map(ms).sum::<f64>() / v.len() as f64;
    let most = |v: &[Duration]| v.iter().map(ms).fold(0.0, f64::max);
    eprintln!(
        "step with no phone: mean {:.2} ms, longest {:.2} ms; with a phone reporting every second: mean {:.2} ms, longest {:.2} ms; \
         the link's turn between two steps: mean {:.2} ms, longest {:.2} ms; counted in 30 s: quiet {quiet_counted} s, talking {counted} s",
        mean(&quiet_steps),
        most(&quiet_steps),
        mean(&steps),
        most(&steps),
        mean(&turns),
        most(&turns),
    );
    // Counting is the same with a phone talking, to the second a step
    // boundary may move.
    assert!(quiet_counted.abs_diff(counted) <= 1, "{quiet_counted} and {counted}");
    assert!(counted >= 29, "{counted}");
    assert!(mean(&steps) < 50.0, "a step is slow: {:.2} ms", mean(&steps));
}
