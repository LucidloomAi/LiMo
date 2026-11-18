//! The ways out, told as short stories: extra time given at the master,
//! requests for more time, one time codes, the guardian PIN, and what a
//! guardian can do at once from the master. Like `accounting.rs`, each test
//! plays one situation step by step with a master, a PC and a phone.

use limo_core::{
    CivilDate, DailyBudget, Decision, DeviceError, DeviceKind, DeviceState, ExtraSource, Grant,
    GrantOutcome, GuardianMode, LockReason, MasterBook, MasterError, OneTimeCode, PinAction,
    PinRecord, Report, RequestAnswer, RequestReply, RuleSet, TryOutcome, WeekBudgets, code_digits,
    next_code,
};

const MIN: u64 = 60;
const INDIA: i32 = 19_800;

/// Home time in India, November 2026 (the 18th is a Wednesday).
fn at(day: u8, hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, INDIA).calendar().at(CivilDate::new(2026, 11, day).unwrap(), hour, minute, 0)
}

fn rules(minutes: u32) -> RuleSet {
    let mut r = RuleSet::new(1, INDIA);
    r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes));
    r.always_allowed_apps = vec!["clock".into()];
    r.blocked_apps = vec!["casino".into()];
    r
}

fn phone_secret() -> Vec<u8> {
    (1u8..=32).collect()
}

fn pc_secret() -> Vec<u8> {
    (101u8..=132).collect()
}

fn salt() -> Vec<u8> {
    (40u8..56).collect()
}

/// A PIN record that is quick to check, for the stories that try many PINs.
/// One story below uses the full cost.
fn quick_pin(pin: &str) -> PinRecord {
    PinRecord::with_iterations(pin, salt(), 1_000).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Pc,
    Phone,
}

struct Home {
    master: MasterBook,
    pc: DeviceState,
    phone: DeviceState,
    phone_home: bool,
}

/// A master, a PC and a phone, paired at `now`, each with its code secret.
fn home(rules: RuleSet, now: i64) -> Home {
    let mut master = MasterBook::new("asha", rules).unwrap();
    let mut pc = DeviceState::from_pairing(&master.add_device("pc", DeviceKind::OnMaster, now).unwrap(), now);
    let mut phone = DeviceState::from_pairing(&master.add_device("phone", DeviceKind::Portable, now).unwrap(), now);
    master.set_code_secret("pc", pc_secret()).unwrap();
    master.set_code_secret("phone", phone_secret()).unwrap();
    pc.set_code_secret(pc_secret()).unwrap();
    phone.set_code_secret(phone_secret()).unwrap();
    Home { master, pc, phone, phone_home: true }
}

impl Home {
    fn device(&mut self, who: Who) -> &mut DeviceState {
        match who {
            Who::Pc => &mut self.pc,
            Who::Phone => &mut self.phone,
        }
    }

    fn reachable(&self, who: Who) -> bool {
        who == Who::Pc || self.phone_home
    }

    /// One report and its grant, both delivered, and the report again at
    /// once when the grant asks for it.
    fn exchange(&mut self, who: Who, now: i64, want_more: bool) -> Grant {
        let report = self.device(who).report(now, want_more);
        let first = self.master.handle_report(&report, now).unwrap().grant;
        assert_eq!(self.device(who).take_grant(&first, now), GrantOutcome::Taken);
        let mut grant = first.clone();
        for _ in 0..3 {
            if !grant.acknowledge_now {
                break;
            }
            let report = self.device(who).report(now, want_more);
            grant = self.master.handle_report(&report, now).unwrap().grant;
            assert_eq!(self.device(who).take_grant(&grant, now), GrantOutcome::Taken);
        }
        first
    }

    /// The hub: asks every device the master wants to hear from, and can
    /// reach, to report, until nothing more moves.
    fn hub(&mut self, now: i64) {
        for _ in 0..20 {
            let day = self.master.today(now);
            let mut moved = false;
            for id in self.master.wanted_reports(now) {
                let who = if id == "pc" { Who::Pc } else { Who::Phone };
                if !self.reachable(who) {
                    continue;
                }
                let waiting = self.master.device_view(&id, day).unwrap().waiting_secs > 0;
                self.exchange(who, now, waiting);
                moved = true;
            }
            if !moved {
                return;
            }
        }
        panic!("the hub kept moving without settling");
    }

    /// Uses a device from `from` until `to`, asking the master when the
    /// decision says so and it is in reach, waiting at the lock screen while
    /// locked for lack of time. Returns the counted seconds used and the last
    /// decision.
    fn play(&mut self, who: Who, from: i64, to: i64) -> (u64, Decision) {
        let mut t = from;
        let mut used = 0;
        loop {
            let mut d = self.device(who).decide(t, Some("game"));
            if d.ask_master {
                if self.reachable(who) {
                    self.exchange(who, t, true);
                    self.hub(t);
                    d = self.device(who).decide(t, Some("game"));
                } else {
                    self.device(who).lost_contact();
                }
            }
            if t >= to {
                return (used, d);
            }
            if !d.allowed {
                let reason = d.device_lock.map(|l| l.reason);
                if !reason.is_some_and(LockReason::is_lack_of_time) {
                    return (used, d);
                }
                t = d.next_check_if_idle_at.min(to);
                continue;
            }
            let until = d.next_check_at.min(to);
            used += self.device(who).record_use("game", t, until).iter().filter(|s| s.counts).map(|s| s.secs()).sum::<u64>();
            t = until;
        }
    }
}

fn reason(d: &Decision) -> Option<LockReason> {
    d.device_lock.map(|l| l.reason)
}

fn added_secs(outcome: &TryOutcome) -> u64 {
    match outcome {
        TryOutcome::Added { added } => added.secs,
        other => panic!("expected time added, got {other:?}"),
    }
}

/// A well formed code that is not right for the phone on Wednesday.
fn wrong_code(h: &Home, day: i64) -> String {
    let right = next_code(phone_secret(), "phone", day, 30, vec![]).unwrap();
    let mut wrong = right.digits.clone();
    let last = wrong.pop().unwrap();
    wrong.push(if last == '9' { '0' } else { char::from(last as u8 + 1) });
    assert_ne!(wrong, right.digits);
    let _ = h;
    wrong
}

#[test]
fn extra_time_given_twice_with_the_same_id_counts_once() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    // The guardian taps "30 more minutes" and the screen sends it twice.
    assert_eq!(h.master.give_extra("wed-evening", 30, at(18, 12, 0)), Ok(true));
    assert_eq!(h.master.give_extra("wed-evening", 30, at(18, 12, 1)), Ok(false));
    assert_eq!(h.master.pool(wed).extra_secs, 30 * MIN);
    let pieces = h.master.extra_time(wed);
    assert_eq!(pieces.len(), 1);
    assert_eq!((pieces[0].id.as_str(), pieces[0].secs, &pieces[0].source), ("wed-evening", 30 * MIN, &ExtraSource::Guardian));
    assert_eq!(h.master.give_extra("x", 0, at(18, 12, 2)), Err(MasterError::MinutesOutOfRange { minutes: 0 }));
    // The PC uses the whole day: the budget and the extra, once.
    let (pc, d) = h.play(Who::Pc, at(18, 13, 0), at(18, 20, 0));
    assert_eq!(pc, 90 * MIN);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
}

#[test]
fn a_request_made_on_the_phone_while_away_reaches_the_master_later_and_is_answered() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    // At school the phone spends its away share, then asks for more.
    h.phone_home = false;
    let (away, d) = h.play(Who::Phone, at(18, 10, 0), at(18, 11, 0));
    assert_eq!(away, 30 * MIN);
    assert_eq!(reason(&d), Some(LockReason::NeedsMaster));
    assert!(d.can_ask_for_time);
    let request = h.phone.ask_for_time(30, "  For the school project  ", at(18, 11, 0)).unwrap();
    assert_eq!(request.reason, "For the school project");
    assert_eq!(request.id, "phone/ask/1");
    // One request waits at a time, and the lock screen stops offering it.
    assert_eq!(h.phone.ask_for_time(10, "", at(18, 11, 1)), Err(DeviceError::RequestWaiting));
    assert!(!h.phone.decide(at(18, 11, 1), Some("game")).can_ask_for_time);
    let seen = h.phone.requests_today(at(18, 11, 1));
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].answer, seen[0].reached_master), (RequestAnswer::Waiting, false));
    // The PC at home uses the free half hour meanwhile.
    let (pc, _) = h.play(Who::Pc, at(18, 12, 0), at(18, 13, 0));
    assert_eq!(pc, 30 * MIN);
    // At 17:00 the phone comes home and reports: the request arrives.
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 17, 0), false);
    let waiting = h.master.waiting_requests(at(18, 17, 0));
    assert_eq!(waiting.len(), 1);
    assert_eq!((waiting[0].request.id.as_str(), waiting[0].request.minutes), ("phone/ask/1", 30));
    assert!(h.phone.requests_today(at(18, 17, 0))[0].reached_master);
    // The guardian grants 20 of the 30 minutes. A request is answered once.
    let view = h.master.answer_request("phone/ask/1", RequestReply::Grant { minutes: 20 }, at(18, 17, 5)).unwrap();
    assert_eq!(view.answer, RequestAnswer::Granted { minutes: 20 });
    assert_eq!(
        h.master.answer_request("phone/ask/1", RequestReply::Refuse, at(18, 17, 6)),
        Err(MasterError::AlreadyAnswered { request_id: "phone/ask/1".into() })
    );
    let pieces = h.master.extra_time(wed);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].source, ExtraSource::Request { request_id: "phone/ask/1".into() });
    assert_eq!(h.master.pool(wed).extra_secs, 20 * MIN);
    // The answer reaches both devices: the hub asks each to report.
    assert_eq!(h.master.wanted_reports(at(18, 17, 5)), vec!["pc".to_string(), "phone".to_string()]);
    h.hub(at(18, 17, 5));
    assert!(h.master.wanted_reports(at(18, 17, 5)).is_empty());
    for device in [&h.pc, &h.phone] {
        let seen = device.requests_today(at(18, 17, 5));
        assert_eq!(seen.len(), 1, "{}", device.device_id());
        assert_eq!(seen[0].answer, RequestAnswer::Granted { minutes: 20 });
    }
    assert!(h.phone.decide(at(18, 17, 6), Some("game")).can_ask_for_time);
    // The phone gets the 20 minutes with its next leases.
    let (more, d) = h.play(Who::Phone, at(18, 17, 10), at(18, 18, 0));
    assert_eq!(more, 20 * MIN);
    assert_eq!(reason(&d), Some(LockReason::TimeUsedUp));
    assert_eq!(away + pc + more, 60 * MIN + 20 * MIN);
}

#[test]
fn a_request_is_answered_only_while_its_day_lasts() {
    let mut h = home(rules(60), at(17, 20, 0));
    let request = h.phone.ask_for_time(15, "", at(18, 20, 0)).unwrap();
    h.exchange(Who::Phone, at(18, 20, 1), false);
    assert_eq!(
        h.master.answer_request(&request.id, RequestReply::Grant { minutes: 15 }, at(19, 9, 0)),
        Err(MasterError::RequestTooLate { request_id: request.id.clone() })
    );
    assert_eq!(
        h.master.answer_request("phone/ask/99", RequestReply::Refuse, at(18, 20, 2)),
        Err(MasterError::UnknownRequest { request_id: "phone/ask/99".into() })
    );
    let refused = h.master.answer_request(&request.id, RequestReply::Refuse, at(18, 20, 3)).unwrap();
    assert_eq!(refused.answer, RequestAnswer::Refused);
    assert_eq!(h.master.pool(h.master.today(at(18, 20, 3))).extra_secs, 0);
    // Asking is bounded: minutes from 1 to 240, a short reason, five a day.
    assert_eq!(h.pc.ask_for_time(0, "", at(18, 20, 4)), Err(DeviceError::MinutesOutOfRange { minutes: 0, most: 240 }));
    let long: String = "a".repeat(201);
    assert_eq!(h.pc.ask_for_time(10, &long, at(18, 20, 4)), Err(DeviceError::ReasonTooLong { chars: 201 }));
}

#[test]
fn a_code_used_on_the_phone_at_school_is_counted_once_at_home() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    // At school the phone spends its away share.
    h.phone_home = false;
    let (away, d) = h.play(Who::Phone, at(18, 9, 0), at(18, 10, 0));
    assert_eq!(away, 30 * MIN);
    assert!(d.can_use_code && !d.allowed);
    // The guardian, at work, makes a code with the secret their phone was
    // given at home, and reads it out over a call.
    let secret = h.master.code_secret("phone").unwrap().unwrap();
    let code: OneTimeCode = next_code(secret, "phone", wed, 30, vec![]).unwrap();
    assert_eq!((code.selector, code.counter), (10, 0));
    let outcome = h.phone.try_code(&code.shown, at(18, 10, 5));
    assert_eq!(added_secs(&outcome), 30 * MIN);
    if let TryOutcome::Added { added } = &outcome {
        assert_eq!(added.id, format!("phone/code/{wed}/10"));
    }
    // The same code works once.
    assert_eq!(h.phone.try_code(&code.digits, at(18, 10, 6)), TryOutcome::AlreadyUsed);
    assert_eq!(h.phone.wrong_guesses(), 0);
    let (more, d) = h.play(Who::Phone, at(18, 10, 10), at(18, 11, 0));
    assert_eq!(more, 30 * MIN);
    assert_eq!(reason(&d), Some(LockReason::NeedsMaster));
    // At home the phone reports, and the report is repeated: the code's time
    // joins Wednesday's extra time once.
    h.phone_home = true;
    let report = h.phone.report(at(18, 17, 0), false);
    assert_eq!(report.added.len(), 1);
    for _ in 0..3 {
        h.master.handle_report(&report, at(18, 17, 0)).unwrap();
    }
    h.exchange(Who::Phone, at(18, 17, 1), false);
    assert!(h.phone.unsent_added().is_empty());
    let pieces = h.master.extra_time(wed);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].source, ExtraSource::Code { device_id: "phone".into(), selector: 10 });
    let pool = h.master.pool(wed);
    assert_eq!((pool.extra_secs, pool.used_secs), (30 * MIN, 60 * MIN));
    assert_eq!(h.master.device_view("phone", wed).unwrap().added_secs, 30 * MIN);
    // The PC gets the rest: budget and extra, never more.
    let (pc, _) = h.play(Who::Pc, at(18, 18, 0), at(18, 20, 0));
    assert_eq!(pc, 30 * MIN);
    assert_eq!(away + more + pc, 60 * MIN + 30 * MIN);
}

#[test]
fn the_phones_code_typed_on_the_pc_is_a_wrong_guess_there() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    let code = next_code(phone_secret(), "phone", wed, 15, vec![]).unwrap();
    assert_eq!(h.pc.try_code(&code.shown, at(18, 12, 0)), TryOutcome::Wrong { wrong_guesses: 1, wait_until: None });
    h.exchange(Who::Pc, at(18, 12, 1), false);
    assert_eq!(h.master.device_view("pc", wed).unwrap().wrong_guesses, 1);
    // It still works on the phone it was made for.
    assert_eq!(added_secs(&h.phone.try_code(&code.shown, at(18, 12, 2))), 15 * MIN);
}

#[test]
fn a_code_from_yesterday_is_a_wrong_guess_today() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    let yesterday = next_code(phone_secret(), "phone", wed - 1, 60, vec![]).unwrap();
    assert_eq!(
        h.phone.try_code(&yesterday.shown, at(18, 9, 0)),
        TryOutcome::Wrong { wrong_guesses: 1, wait_until: None }
    );
    // A device without a code secret refuses codes without counting them.
    let mut bare = DeviceState::from_pairing(&h.master.add_device("tablet", DeviceKind::Portable, at(18, 9, 0)).unwrap(), at(18, 9, 0));
    assert_eq!(bare.try_code(&yesterday.shown, at(18, 9, 1)), TryOutcome::NotSet);
    assert_eq!(bare.set_code_secret(vec![1; 31]), Err(DeviceError::SecretWrongLength { given: 31 }));
    assert_eq!(bare.wrong_guesses(), 0);
}

#[test]
fn all_ten_codes_of_a_day_work_once_each_and_the_eleventh_cannot_be_made() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    let mut made = Vec::new();
    let mut codes = Vec::new();
    for (i, minutes) in [15, 30, 60, 15, 30, 60, 15, 30, 60, 15].into_iter().enumerate() {
        let code = next_code(phone_secret(), "phone", wed, minutes, made.clone()).unwrap();
        assert_eq!(code.counter as usize, i);
        made.push(code.counter);
        codes.push(code);
    }
    let digits: std::collections::BTreeSet<&str> = codes.iter().map(|c| c.digits.as_str()).collect();
    assert_eq!(digits.len(), 10);
    let err = next_code(phone_secret(), "phone", wed, 30, made).unwrap_err();
    assert!(err.to_string().contains("All ten codes"), "{err}");
    let mut total = 0;
    for (i, code) in codes.iter().enumerate() {
        total += added_secs(&h.phone.try_code(&code.shown, at(18, 12, i as u8)));
    }
    assert_eq!(total, (4 * 15 + 3 * 30 + 3 * 60) * MIN);
    for code in &codes {
        assert_eq!(h.phone.try_code(&code.digits, at(18, 12, 30)), TryOutcome::AlreadyUsed);
    }
    h.exchange(Who::Phone, at(18, 12, 31), false);
    assert_eq!(h.master.extra_time(wed).len(), 10);
    assert_eq!(h.master.pool(wed).extra_secs, total);
    assert_eq!(h.phone.wrong_guesses(), 0);
}

#[test]
fn the_pin_adds_time_on_a_phone_with_no_network() {
    // The full cost of the PIN hash, as in the product.
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    assert_eq!(h.master.set_pin("864213", salt()), Ok(1));
    // The phone gets the record with its next grant.
    assert!(!h.phone.has_pin());
    let grant = h.exchange(Who::Phone, at(17, 21, 0), false);
    assert_eq!(grant.pin.as_ref().map(|p| p.version), Some(1));
    assert!(h.phone.has_pin() && h.phone.pin_version() == 1);
    // Once the phone has said it holds it, grants stop carrying it.
    let grant = h.exchange(Who::Phone, at(17, 21, 1), false);
    assert!(grant.pin.is_none());
    // Out of reach, with its away share spent, the phone is opened by the
    // guardian who is with the child.
    h.phone_home = false;
    let (away, d) = h.play(Who::Phone, at(18, 15, 0), at(18, 16, 0));
    assert_eq!(away, 30 * MIN);
    assert!(d.can_use_pin && !d.allowed);
    let outcome = h.phone.try_pin("864213", PinAction::AddMinutes { minutes: 20 }, at(18, 16, 0)).unwrap();
    assert_eq!(added_secs(&outcome), 20 * MIN);
    if let TryOutcome::Added { added } = &outcome {
        assert_eq!(added.id, "phone/pin/1");
    }
    let (more, _) = h.play(Who::Phone, at(18, 16, 0), at(18, 17, 0));
    assert_eq!(more, 20 * MIN);
    // At home it is counted once.
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 18, 0), false);
    h.exchange(Who::Phone, at(18, 18, 1), false);
    let pieces = h.master.extra_time(wed);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].source, ExtraSource::Pin { device_id: "phone".into() });
    assert_eq!(h.master.pool(wed).extra_secs, 20 * MIN);
    // Release is confirmed, and nothing else changes.
    let release = h.phone.try_pin("864213", PinAction::ConfirmRelease, at(18, 18, 2)).unwrap();
    assert_eq!(release, TryOutcome::ReleaseConfirmed);
    // Actions that could not be done are refused before the PIN is looked
    // at, and do not count.
    assert_eq!(
        h.phone.try_pin("0000", PinAction::AddMinutes { minutes: 0 }, at(18, 18, 3)),
        Err(DeviceError::MinutesOutOfRange { minutes: 0, most: 1_440 })
    );
    assert_eq!(
        h.phone.try_pin("0000", PinAction::UnlockUntil { until: at(18, 18, 3) }, at(18, 18, 3)),
        Err(DeviceError::UnlockOutOfRange)
    );
    assert_eq!(h.phone.wrong_guesses(), 0);
}

#[test]
fn wrong_guesses_set_a_growing_wait_that_survives_a_restart_and_a_clock_set_back() {
    let mut h = home(rules(60), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    h.master.set_pin_record(quick_pin("246813")).unwrap();
    h.exchange(Who::Phone, at(18, 11, 0), false);
    let wrong = wrong_code(&h, wed);
    let t0 = at(18, 12, 0);
    // Four wrong codes and a wrong PIN: the fifth wrong guess sets a minute.
    for n in 1..=4 {
        assert_eq!(h.phone.try_code(&wrong, t0), TryOutcome::Wrong { wrong_guesses: n, wait_until: None });
    }
    let fifth = h.phone.try_pin("1111", PinAction::ConfirmRelease, t0).unwrap();
    assert_eq!(fifth, TryOutcome::Wrong { wrong_guesses: 5, wait_until: Some(t0 + 60) });
    // During the wait nothing is tried, so nothing is counted; a code that
    // is not 8 digits is refused without counting as well.
    assert_eq!(h.phone.try_code(&wrong, t0 + 10), TryOutcome::MustWait { until: t0 + 60 });
    assert_eq!(h.phone.try_pin("246813", PinAction::ConfirmRelease, t0 + 10), Ok(TryOutcome::MustWait { until: t0 + 60 }));
    assert_eq!(h.phone.try_code("1234", t0 + 10), TryOutcome::Malformed);
    assert_eq!(h.phone.try_pin("12", PinAction::ConfirmRelease, t0 + 10), Ok(TryOutcome::Malformed));
    assert_eq!(h.phone.wrong_guesses(), 5);
    let d = h.phone.decide(t0 + 10, None);
    assert_eq!(d.try_wait_until, Some(t0 + 60));
    // The phone restarts: the wait is still there.
    let saved = serde_json::to_string(&h.phone).unwrap();
    h.phone = serde_json::from_str(&saved).unwrap();
    assert_eq!(h.phone.try_code(&wrong, t0 + 20), TryOutcome::MustWait { until: t0 + 60 });
    // The clock is set back an hour: the wait ends at the same moment, so it
    // is longer, never shorter.
    assert_eq!(h.phone.try_code(&wrong, t0 - 3_600), TryOutcome::MustWait { until: t0 + 60 });
    // Each further wrong guess doubles the wait, up to an hour.
    let mut t = t0 + 60;
    for (n, wait) in [(6, 120), (7, 240), (8, 480), (9, 960), (10, 1_920), (11, 3_600), (12, 3_600)] {
        assert_eq!(h.phone.try_code(&wrong, t), TryOutcome::Wrong { wrong_guesses: n, wait_until: Some(t + wait) });
        assert_eq!(h.phone.try_code(&wrong, t + wait - 1), TryOutcome::MustWait { until: t + wait });
        t += wait;
    }
    // The guardian can see it at home.
    h.exchange(Who::Phone, t, false);
    assert_eq!(h.master.device_view("phone", wed).unwrap().wrong_guesses, 12);
    // A right code adds its time but leaves the count as it is.
    let code = next_code(phone_secret(), "phone", wed, 15, vec![]).unwrap();
    assert_eq!(added_secs(&h.phone.try_code(&code.shown, t)), 15 * MIN);
    assert_eq!(h.phone.wrong_guesses(), 12);
    assert_eq!(h.phone.try_code(&wrong, t), TryOutcome::Wrong { wrong_guesses: 13, wait_until: Some(t + 3_600) });
    // A right PIN, after the wait, clears the count and the wait.
    t += 3_600;
    assert_eq!(h.phone.try_pin("246813", PinAction::ConfirmRelease, t), Ok(TryOutcome::ReleaseConfirmed));
    assert_eq!(h.phone.wrong_guesses(), 0);
    assert_eq!(h.phone.decide(t, None).try_wait_until, None);
    h.exchange(Who::Phone, t, false);
    assert_eq!(h.master.device_view("phone", wed).unwrap().wrong_guesses, 0);
}

#[test]
fn a_changed_pin_replaces_the_old_one_on_every_device_that_hears_of_it() {
    let mut h = home(rules(60), at(17, 20, 0));
    assert_eq!(h.master.set_pin_record(quick_pin("246813")), Ok(1));
    h.exchange(Who::Phone, at(18, 9, 0), false);
    h.exchange(Who::Pc, at(18, 9, 0), false);
    h.phone_home = false;
    // The guardian changes the PIN while the phone is at school.
    assert_eq!(h.master.set_pin_record(quick_pin("975312")), Ok(2));
    assert!(h.master.wanted_reports(at(18, 10, 0)).contains(&"pc".to_string()));
    h.hub(at(18, 10, 0));
    assert_eq!(h.pc.pin_version(), 2);
    let unlock = PinAction::UnlockUntil { until: at(18, 11, 0) };
    // On the PC the old PIN is now wrong and the new one right.
    assert_eq!(h.pc.try_pin("246813", unlock, at(18, 10, 1)), Ok(TryOutcome::Wrong { wrong_guesses: 1, wait_until: None }));
    assert_eq!(h.pc.try_pin("975312", unlock, at(18, 10, 2)), Ok(TryOutcome::Unlocked { until: at(18, 11, 0) }));
    // The phone away still holds the old record until it hears.
    assert_eq!(h.phone.pin_version(), 1);
    assert_eq!(h.phone.try_pin("975312", unlock, at(18, 10, 3)), Ok(TryOutcome::Wrong { wrong_guesses: 1, wait_until: None }));
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 10, 4), false);
    assert_eq!(h.phone.pin_version(), 2);
    assert_eq!(h.phone.try_pin("246813", unlock, at(18, 10, 5)), Ok(TryOutcome::Wrong { wrong_guesses: 2, wait_until: None }));
    assert_eq!(h.phone.try_pin("975312", unlock, at(18, 10, 6)), Ok(TryOutcome::Unlocked { until: at(18, 11, 0) }));
    // Removing the PIN reaches the devices too: they refuse PIN actions.
    assert_eq!(h.master.clear_pin(), 3);
    h.hub(at(18, 10, 7));
    assert!(!h.phone.has_pin());
    assert_eq!(h.phone.try_pin("975312", unlock, at(18, 10, 8)), Ok(TryOutcome::NotSet));
    // A PIN of the wrong shape is refused by the master too.
    assert!(matches!(h.master.set_pin("12a4", salt()), Err(MasterError::BadPin { .. })));
}

#[test]
fn a_lock_from_the_master_reaches_every_device_and_lifting_it_opens_them() {
    let mut h = home(rules(120), at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    h.exchange(Who::Phone, at(18, 15, 0), true);
    let setting = h.master.lock_now(None, at(18, 15, 30)).unwrap();
    assert_eq!(setting.mode, GuardianMode::Lock { until: None });
    // The hub asks both devices to report, and both lock.
    assert_eq!(h.master.wanted_reports(at(18, 15, 30)), vec!["pc".to_string(), "phone".to_string()]);
    h.hub(at(18, 15, 30));
    for who in [Who::Pc, Who::Phone] {
        let d = h.device(who).decide(at(18, 15, 31), Some("game"));
        assert_eq!(reason(&d), Some(LockReason::GuardianLock));
        assert_eq!(d.lock.unwrap().until, None);
        assert!(!d.ask_master && !d.can_ask_for_time);
        // The clock, always allowed, still works and does not count.
        let clock = h.device(who).decide(at(18, 15, 31), Some("clock"));
        assert!(clock.allowed && !clock.counts);
    }
    // Under the lock no counted use is recorded.
    let (used, _) = h.play(Who::Pc, at(18, 15, 31), at(18, 16, 30));
    assert_eq!(used, 0);
    assert_eq!(h.master.pool(wed).used_secs, 0);
    // A lock that ends in the past is refused.
    assert_eq!(h.master.lock_now(Some(at(18, 15, 0)), at(18, 16, 0)), Err(MasterError::TimeOutOfRange));
    h.master.lift(at(18, 16, 30));
    h.hub(at(18, 16, 30));
    let (pc, _) = h.play(Who::Pc, at(18, 16, 31), at(18, 16, 41));
    assert_eq!(pc, 10 * MIN);
}

#[test]
fn an_unlock_from_the_master_keeps_counting_and_the_master_counts_it_all() {
    let mut r = rules(60);
    r.blocked_hours = vec![limo_core::BlockedWindow {
        days: limo_core::Weekday::ALL.to_vec(),
        start_minute: 21 * 60,
        end_minute: 7 * 60,
    }];
    let mut h = home(r, at(17, 20, 0));
    let wed = h.master.today(at(18, 12, 0));
    h.phone_home = false;
    // The PC uses its half hour; the phone holds the other half away.
    let (pc, d) = h.play(Who::Pc, at(18, 20, 0), at(18, 20, 40));
    assert_eq!(pc, 30 * MIN);
    assert_eq!(reason(&d), Some(LockReason::HeldByAnotherDevice));
    // A family film night: the guardian unlocks until 22:00, past bedtime.
    assert_eq!(h.master.unlock_until(at(19, 22, 0), at(18, 20, 40)), Err(MasterError::TimeOutOfRange));
    h.master.unlock_until(at(18, 22, 0), at(18, 20, 40)).unwrap();
    h.hub(at(18, 20, 40));
    let d = h.pc.decide(at(18, 20, 41), Some("game"));
    assert!(d.allowed && d.counts);
    assert_eq!(d.unlocked_until, Some(at(18, 22, 0)));
    let (film, d) = h.play(Who::Pc, at(18, 20, 41), at(18, 22, 0));
    assert_eq!(film, 79 * MIN);
    // Bedtime and no time left: locked when the unlock ends.
    let d2 = h.pc.decide(at(18, 22, 0), Some("game"));
    assert_eq!(reason(&d2), Some(LockReason::BlockedHours));
    let _ = d;
    // The master counted all of it: the day is over its budget by the film,
    // and nothing more is handed out.
    let pool = h.master.pool(wed);
    assert_eq!(pool.used_secs, pc + film);
    assert_eq!(pool.free_secs, Some(0));
    assert!(pool.over_secs >= film);
    assert_eq!(h.pc.holding_at(at(18, 22, 0)).over_secs(), 79 * MIN);
}

#[test]
fn a_pin_unlock_on_the_phone_beats_an_older_lock_from_the_master_until_it_ends() {
    let mut h = home(rules(120), at(17, 20, 0));
    h.master.set_pin_record(quick_pin("246813")).unwrap();
    h.master.lock_now(None, at(18, 9, 0)).unwrap();
    h.hub(at(18, 9, 0));
    h.phone_home = false;
    assert_eq!(reason(&h.phone.decide(at(18, 10, 0), Some("game"))), Some(LockReason::GuardianLock));
    // Away from home, a guardian with the PIN opens the phone for an hour.
    let unlocked = h.phone.try_pin("246813", PinAction::UnlockUntil { until: at(18, 11, 0) }, at(18, 10, 0)).unwrap();
    assert_eq!(unlocked, TryOutcome::Unlocked { until: at(18, 11, 0) });
    let d = h.phone.decide(at(18, 10, 1), Some("game"));
    assert!(d.allowed);
    assert_eq!(d.unlocked_until, Some(at(18, 11, 0)));
    let (used, _) = h.play(Who::Phone, at(18, 10, 1), at(18, 11, 0));
    assert_eq!(used, 59 * MIN);
    // When it ends, the lock from the master applies again.
    assert_eq!(reason(&h.phone.decide(at(18, 11, 0), Some("game"))), Some(LockReason::GuardianLock));
    // At home the master sees the unlock while it lasts; lifting ends both.
    h.phone.try_pin("246813", PinAction::UnlockUntil { until: at(18, 13, 0) }, at(18, 12, 0)).unwrap();
    h.phone_home = true;
    h.exchange(Who::Phone, at(18, 12, 1), false);
    let wed = h.master.today(at(18, 12, 1));
    assert_eq!(h.master.device_view("phone", wed).unwrap().pin_unlocked_until, Some(at(18, 13, 0)));
    h.master.lift(at(18, 12, 2));
    h.hub(at(18, 12, 2));
    let d = h.phone.decide(at(18, 12, 3), Some("game"));
    assert_eq!(d.unlocked_until, None);
    assert!(d.allowed, "the rules alone decide again, and time is in hand");
}

#[test]
fn state_saved_by_piece_one_still_loads_and_goes_on_working() {
    // Saved with the code at the commit "Add the rules core", before any of
    // the ways out existed.
    let book: MasterBook = serde_json::from_str(include_str!("fixtures/piece_one_master_book.json")).unwrap();
    let mut phone: DeviceState = serde_json::from_str(include_str!("fixtures/piece_one_phone_state.json")).unwrap();
    let mut pc: DeviceState = serde_json::from_str(include_str!("fixtures/piece_one_pc_state.json")).unwrap();
    let report: Report = serde_json::from_str(include_str!("fixtures/piece_one_report.json")).unwrap();
    let grant: Grant = serde_json::from_str(include_str!("fixtures/piece_one_grant.json")).unwrap();
    let mut book = book;
    // The pool reads as it did then.
    let day = 20_775;
    let pool = book.pool(day);
    assert_eq!(
        (pool.budget_secs, pool.extra_secs, pool.used_secs, pool.reserved_secs, pool.free_secs),
        (Some(7_200), 900, 4_000, Some(3_500), Some(600))
    );
    let pieces = book.extra_time(day);
    assert_eq!((pieces.len(), pieces[0].id.as_str(), pieces[0].secs), (1, "earlier", 900));
    // Nothing of the ways out is set.
    assert!(!phone.has_pin() && !phone.has_code_secret() && phone.wrong_guesses() == 0);
    assert_eq!(book.guardian_setting().mode, GuardianMode::Nothing);
    assert!(report.added.is_empty() && grant.pin.is_none());
    // Both sides go on: the old report is repeated, the devices report anew.
    let now = 1_794_976_800 + 60;
    book.handle_report(&report, now).unwrap();
    for device in [&mut phone, &mut pc] {
        let g = book.handle_report(&device.report(now, true), now).unwrap().grant;
        assert_eq!(device.take_grant(&g, now), GrantOutcome::Taken);
        let d = device.decide(now, Some("game"));
        assert!(d.next_check_at > now);
    }
    assert_eq!(phone.take_grant(&grant, now), GrantOutcome::Stale);
    let pool = book.pool(day);
    assert!(pool.used_secs + pool.reserved_secs.unwrap() <= 7_200 + 900);
    // And the ways out work on old state.
    phone.set_code_secret(phone_secret()).unwrap();
    let code = next_code(phone_secret(), "phone", day, 15, vec![]).unwrap();
    assert_eq!(added_secs(&phone.try_code(&code.shown, now)), 15 * MIN);
}

#[test]
fn secrets_and_pins_never_show_in_debug_output_or_errors() {
    let mut h = home(rules(60), at(17, 20, 0));
    h.master.set_pin_record(quick_pin("864209")).unwrap();
    let grant = h.exchange(Who::Phone, at(18, 9, 0), false);
    let record = quick_pin("864209");
    let hash_hex: String = record.hash.iter().map(|b| format!("{b:02x}")).collect();
    let texts = [
        format!("{:?}", h.master),
        format!("{:?}", h.phone),
        format!("{grant:?}"),
        format!("{record:?}"),
    ];
    // The phone's secret is 1, 2, ..., 32 and the PC's 101 to 132: a run of
    // them would show as "1, 2, 3" or "101, 102".
    for text in &texts {
        assert!(!text.contains("1, 2, 3") && !text.contains("101, 102"), "{text}");
        assert!(!text.contains("864209"), "{text}");
        assert!(!text.contains(&hash_hex[..8]), "{text}");
        assert!(!text.contains("40, 41, 42"), "{text}");
    }
    assert!(texts[3].contains("hidden"));
    // Errors say what is wrong, never what was typed.
    let err = h.master.set_pin("86x209", salt()).unwrap_err().to_string();
    assert!(!err.contains("86x209") && err.ends_with('.'), "{err}");
    for err in [
        DeviceError::SecretWrongLength { given: 3 }.to_string(),
        DeviceError::RequestWaiting.to_string(),
        MasterError::TimeOutOfRange.to_string(),
    ] {
        assert!(err.ends_with('.') && !err.contains(" - ") && !err.contains('\u{2014}'), "{err}");
    }
    assert_eq!(code_digits("8642 0000"), Some("86420000".into()));
}

#[test]
fn every_new_message_field_survives_a_round_trip_as_json() {
    let mut h = home(rules(60), at(17, 20, 0));
    h.master.set_pin_record(quick_pin("246813")).unwrap();
    h.master.unlock_until(at(18, 12, 0), at(18, 11, 0)).unwrap();
    let wed = h.master.today(at(18, 11, 0));
    h.phone.ask_for_time(20, "Homework", at(18, 11, 0)).unwrap();
    let code = next_code(phone_secret(), "phone", wed, 60, vec![]).unwrap();
    h.phone.try_code(&code.shown, at(18, 11, 0));
    h.phone.try_code("0000 0000", at(18, 11, 0));
    let report = h.phone.report(at(18, 11, 1), true);
    assert!(!report.added.is_empty() && !report.requests.is_empty() && report.wrong_guesses == 1);
    let grant = h.master.handle_report(&report, at(18, 11, 1)).unwrap().grant;
    assert!(grant.pin.is_some() && !grant.requests.is_empty() && grant.news > 0);
    h.phone.take_grant(&grant, at(18, 11, 1));
    macro_rules! round_trip {
        ($value:expr, $t:ty) => {{
            let text = serde_json::to_string(&$value).unwrap();
            let back: $t = serde_json::from_str(&text).unwrap();
            assert_eq!(back, $value);
        }};
    }
    round_trip!(report, Report);
    round_trip!(grant, Grant);
    round_trip!(h.master, MasterBook);
    round_trip!(h.phone, DeviceState);
    round_trip!(h.phone.decide(at(18, 11, 2), Some("game")), Decision);
    round_trip!(h.master.extra_time(wed), Vec<limo_core::ExtraTime>);
    round_trip!(h.master.requests(wed), Vec<limo_core::RequestView>);
    round_trip!(h.phone.standing_at(at(18, 11, 2)), limo_core::Standing);
}

/// How long one PIN check takes at the full cost on the machine it runs on.
/// Not run by default; run it with
/// `cargo test --release --test ways_out -- --ignored --nocapture`.
#[test]
#[ignore]
fn the_pin_hash_cost_on_this_machine() {
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let record = PinRecord::new("12345678", salt()).unwrap();
        times.push(start.elapsed());
        assert_eq!(record.iterations, limo_core::pin::PIN_ITERATIONS);
    }
    times.sort();
    println!(
        "PBKDF2-HMAC-SHA256, {} rounds: {:?} (fastest of 5), {:?} (slowest)",
        limo_core::pin::PIN_ITERATIONS,
        times[0],
        times[4]
    );
}
