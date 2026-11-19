//! The three ways out from the lock, on the phone: asking for more time (at
//! home and from school), a one time code read over a call, and the
//! guardian's PIN; with their words, the wait after wrong tries, and what a
//! restart keeps.

use limo_core::{DailyBudget, DeviceError, PinAction, RequestAnswer, RequestReply, TryOutcome, WeekBudgets};
use limo_phone::{Cover, LockWhy, PhoneError, Way};

use crate::common::*;

/// Mira's phone with `minutes` a day, used up: locked. In reach of the
/// master when `home`.
fn used_up(minutes: u32, home: bool) -> Family {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes));
        r.away_share_percent = 100;
    });
    f.in_reach = home;
    f.run_until_locked();
    f
}

#[test]
fn a_request_made_at_home_reaches_the_master_and_its_answer_the_lock() {
    let mut f = used_up(10, true);
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::TimeUsedUp);
    assert!(lock.can_ask_for_time && lock.latest_request.is_none());
    assert_eq!(lock.ways[0].title, "Ask for more time");
    assert_eq!(lock.ways[0].sub, "A guardian sees your request in LiMo");

    let request = f.way_out(|e, now| e.ask_for_time(20, "For the school project", now)).unwrap();
    // It goes with the next report, at the next call.
    f.tick();
    let now = f.now();
    let waiting = f.hub().waiting_requests(now).unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].request.request.id, request.id);
    let lock = f.lock();
    let line = lock.request_line.clone().unwrap();
    assert_eq!((line.text.as_str(), line.stamp.as_str()), ("You asked for 20 min at 4:10 pm", "Waiting"));
    assert!(!lock.ways.iter().any(|w| w.way == Way::Ask));
    // Asking again while one waits is refused, in the core's words.
    let err = f.way_out(|e, now| e.ask_for_time(10, "", now)).unwrap_err();
    assert_eq!(err, PhoneError::Refused { problem: DeviceError::RequestWaiting });
    assert_eq!(err.to_string(), "A request from this device is still waiting for an answer.");

    // Granted with fifteen: the phone hears of it (prodded), and opens.
    let (child, now) = (f.child.clone(), f.now());
    f.hub().answer_request(&child, &request.id, RequestReply::Grant { minutes: 15 }, now).unwrap();
    f.run(2);
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert_eq!(f.orders.time_left.seconds_left, Some(15 * 60 - 1));
    f.run_until_locked();
    let lock = f.lock();
    assert_eq!(lock.latest_request.map(|r| r.answer), Some(RequestAnswer::Granted { minutes: 15 }));
    assert_eq!(lock.request_line.unwrap().stamp, "Given 15 min");
    assert_eq!(f.counted(), 25 * 60);
}

#[test]
fn a_request_made_away_goes_when_the_phone_is_back_home() {
    let mut f = used_up(10, false);
    let request = f.way_out(|e, now| e.ask_for_time(30, "", now)).unwrap();
    f.run(MIN);
    let now = f.now();
    assert!(f.hub().waiting_requests(now).unwrap().is_empty());
    let line = f.lock().request_line.unwrap();
    assert_eq!(line.stamp, "Waiting");
    // Home: it reaches the master; refused there, and the lock says so.
    f.in_reach = true;
    f.tick();
    let now = f.now();
    assert_eq!(f.hub().waiting_requests(now).unwrap().len(), 1);
    let child = f.child.clone();
    f.hub().answer_request(&child, &request.id, RequestReply::Refuse, now).unwrap();
    f.run(2);
    let lock = f.lock();
    assert_eq!(lock.request_line.unwrap().stamp, "Not this time");
    assert!(lock.can_ask_for_time);
}

#[test]
fn a_one_time_code_adds_time_once_and_wrong_codes_set_a_wait_that_survives_a_restart() {
    // At school, out of reach: a guardian reads a code over a call.
    let mut f = used_up(10, false);
    let lock = f.lock();
    assert!(lock.can_use_code);
    assert_eq!((lock.ways[1].title.as_str(), lock.ways[1].sub.as_str()), (
        "Type a one time code",
        "A guardian who is away can read you 8 digits"
    ));
    let (phone, now) = (f.phone_id.clone(), f.now());
    let code = f.hub().make_code(&phone, 15, now).unwrap();
    let answer = f.way_out(|e, now| e.try_code(&code.shown, now)).unwrap();
    assert!(matches!(answer.outcome, TryOutcome::Added { .. }));
    assert_eq!(answer.message, "15 minutes added.");
    assert_eq!(answer.opens_now.as_deref(), Some("The phone opens now."));
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert_eq!(f.orders.time_left.seconds_left, Some(15 * 60));
    // Typed again: used, and not a wrong guess.
    let again = f.way_out(|e, now| e.try_code(&code.shown, now)).unwrap();
    assert_eq!((again.outcome, again.message.as_str()), (TryOutcome::AlreadyUsed, "That code was already used today."));
    f.run_until_locked();
    assert_eq!(f.counted(), 25 * 60);

    // Five wrong codes: the words count down, and the fifth sets a wait.
    let said: Vec<String> = (0..5).map(|_| f.way_out(|e, now| e.try_code("1234 5678", now)).unwrap().message).collect();
    assert_eq!(said, vec![
        "That code is not right. 4 more tries before a wait.",
        "That code is not right. 3 more tries before a wait.",
        "That code is not right. 2 more tries before a wait.",
        "That code is not right. 1 more try before a wait.",
        "Too many wrong tries. The next try is at 4:26 pm.",
    ]);
    let short = f.way_out(|e, now| e.try_code("12", now)).unwrap();
    assert_eq!((short.outcome, short.message.as_str()), (TryOutcome::Malformed, "A code has 8 digits."));
    f.tick();
    let lock = f.lock();
    assert_eq!(lock.wait_words.as_deref(), Some("Too many wrong tries. The next try is at 4:26 pm."));
    let code_row = lock.ways.iter().find(|w| w.way == Way::Code).unwrap();
    assert!(code_row.off);
    assert_eq!(code_row.sub, "Wait until 4:26 pm");
    // A crash and a restart change nothing: the wait and the count stay.
    f.crash();
    f.tick();
    assert_eq!(f.lock().try_wait_until, lock.try_wait_until);
    let must = f.way_out(|e, now| e.try_code("1234 5678", now)).unwrap();
    assert!(matches!(must.outcome, TryOutcome::MustWait { .. }));
    // Home: the guardian sees the wrong guesses, and the code as extra time.
    f.in_reach = true;
    f.tick();
    let now = f.now();
    let child = f.child.clone();
    let today = f.hub().child_today(&child, now).unwrap();
    assert_eq!(today.devices[0].wrong_guesses, 5);
    assert_eq!(today.extra_secs, 15 * 60);
}

#[test]
fn the_pin_adds_time_unlocks_and_confirms_a_release_with_no_network() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    let now = f.now();
    f.hub().set_pin(PIN, now).unwrap();
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10));
        r.away_share_percent = 100;
        r.blocked_hours = vec![limo_core::BlockedWindow {
            days: limo_core::Weekday::ALL.to_vec(),
            start_minute: 21 * 60,
            end_minute: 7 * 60,
        }];
    });
    f.in_reach = false;
    f.run_until_locked();
    let lock = f.lock();
    assert!(lock.can_use_pin);
    assert_eq!(lock.ways.iter().map(|w| w.way).collect::<Vec<_>>(), vec![Way::Ask, Way::Code, Way::Pin]);
    assert_eq!(lock.ways[2].sub, "For a guardian who is with you");

    let wrong = f.way_out(|e, now| e.try_pin("000000", PinAction::AddMinutes { minutes: 10 }, now)).unwrap();
    assert_eq!(wrong.message, "That PIN is not right. 4 more tries before a wait.");
    let added = f.way_out(|e, now| e.try_pin(PIN, PinAction::AddMinutes { minutes: 10 }, now)).unwrap();
    assert_eq!((added.message.as_str(), added.opens_now.as_deref()), ("10 minutes added.", Some("The phone opens now.")));
    f.tick();
    assert_eq!(f.orders.time_left.seconds_left, Some(600));
    f.run_until_locked();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);

    // Bedtime: only the PIN is offered; a code cannot open it.
    f.run_until(at(21, 0) + 1);
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::BlockedHours);
    assert_eq!(lock.ways.iter().map(|w| w.way).collect::<Vec<_>>(), vec![Way::Pin]);
    assert_eq!(lock.until_words.as_deref(), Some("Tomorrow, 7:00 am"));
    // A guardian unlocks it for half an hour.
    let until = f.wall() + 30 * MIN;
    let open = f.way_out(|e, now| e.try_pin(PIN, PinAction::UnlockUntil { until }, now)).unwrap();
    assert_eq!((open.message.as_str(), open.opens_now.as_deref()), ("Open until 9:31 pm.", Some("The phone opens now.")));
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert_eq!(f.orders.time_left.words, "Open until 9:31 pm");
    f.run_until(until + 1);
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
    // Use under the unlock counted, beyond what the phone held.
    assert_eq!(f.view().over_secs, 30 * 60);
    // An unlock that cannot be: refused before the PIN is looked at.
    let err = f.way_out(|e, now| e.try_pin(PIN, PinAction::UnlockUntil { until: now - 1 }, now)).unwrap_err();
    assert_eq!(err, PhoneError::Refused { problem: DeviceError::UnlockOutOfRange });
    // A release: the engine only says the PIN was right.
    let release = f.way_out(|e, now| e.try_pin(PIN, PinAction::ConfirmRelease, now)).unwrap();
    assert_eq!((release.outcome, release.message.as_str()), (TryOutcome::ReleaseConfirmed, "That was right."));
}
