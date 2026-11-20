//! What is covered and what counts: allowed apps usable under the lock and
//! recorded without counting; nothing ever covered over a ringing call, the
//! diallers or LiMo; a call in progress no way around the limits; a blocked
//! app covered, named, the child sent home, nothing counted and the
//! guardian told once a day; and the rule of what counts.

use limo_core::{DailyBudget, WeekBudgets};
use limo_phone::{CallState, Cover, Front, LockWhy, PhoneNote};

use crate::common::*;

/// Mira's phone at home with 10 minutes a day, a clock app always allowed
/// and a casino blocked, used up: locked.
fn locked_phone() -> Family {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10));
        r.always_allowed_apps = vec![CLOCK.into()];
        r.blocked_apps = vec![CASINO.into()];
    });
    f.run_until_locked();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    f
}

#[test]
fn allowed_apps_stay_usable_under_the_lock_and_do_not_count() {
    let mut f = locked_phone();
    let counted = f.counted();
    let lock = f.lock();
    assert_eq!(lock.allowed_packages, vec![CLOCK.to_string()]);
    // The child opens the clock from the lock: nothing is drawn over it, and
    // its time is recorded but does not count.
    f.front = Front::app(CLOCK);
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(f.orders.time_left.locked && !f.orders.time_left.counting);
    assert_eq!(f.orders.time_left.words, "Today's time is used up.");
    f.run(2 * MIN);
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert_eq!(f.counted(), counted);
    let clock_use: u64 = f.view().unreported.iter().filter(|s| s.app_id == CLOCK).map(|s| s.secs()).sum();
    let reported = {
        let (child, day) = (f.child.clone(), day_of(f.now()));
        f.hub().usage(&child, day, day).unwrap()
    };
    let reported_clock: u64 = reported.apps.iter().filter(|a| a.app_id == CLOCK).map(|a| a.secs).sum();
    assert!(clock_use + reported_clock >= 2 * 60 - 1, "{clock_use} + {reported_clock}");
    assert_eq!(reported.apps.iter().filter(|a| a.app_id == CLOCK).map(|a| a.counted_secs).sum::<u64>(), 0);
    // Back to the home screen, or to any other app: the lock again.
    f.front = Front::Home;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    f.front = Front::app(GAME);
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    // An app the host cannot name is covered too.
    f.front = Front::Unknown;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
}

/// What a cover is, without its words and without the way back to a call.
fn kind(cover: &Cover) -> String {
    match cover {
        Cover::Nothing => "nothing".to_string(),
        Cover::Block { package, .. } => format!("block {package}"),
        Cover::Lock(lock) => format!("lock {:?}", lock.why),
        Cover::Guard(guard) => format!("guard {:?}", guard.page),
    }
}

#[test]
fn a_ringing_call_is_never_covered_nor_the_diallers_limo_or_the_keyguard() {
    let mut f = locked_phone();
    // A call rings while the lock shows: the lock steps aside at once,
    // whatever is in front, and nothing counts.
    f.call = CallState::Ringing;
    for front in [Front::app(GAME), Front::app(CASINO), Front::Home, Front::Unknown, Front::Dialler] {
        f.front = front.clone();
        f.tick();
        assert_eq!(f.orders.cover, Cover::Nothing, "{front:?}");
        assert!(!f.orders.time_left.counting);
        f.run(3);
    }
    f.call = CallState::Idle;
    // The dialler, the emergency dialler and LiMo itself, with no call.
    for front in [Front::Dialler, Front::EmergencyDialler, Front::LiMo] {
        f.front = front.clone();
        f.run(5);
        assert_eq!(f.orders.cover, Cover::Nothing, "{front:?}");
    }
    // Android's own lock screen, and a screen that is off.
    f.front = Front::app(GAME);
    f.keyguard = true;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.keyguard = false;
    f.screen_on = false;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.screen_on = true;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);

    // Every mix of the facts that matter, locked, open and blocked: never a
    // cover over a ringing call, the diallers, LiMo, the keyguard or a dark
    // screen; and during a call in progress, anything else is covered
    // exactly as it would be with no call.
    let mut open = Family::new(at(16, 0));
    open.set_rules(|r| r.blocked_apps = vec![CASINO.into()]);
    for phone in [&mut f, &mut open] {
        for call in [CallState::Idle, CallState::Ringing, CallState::Ongoing] {
            for front in [
                Front::app(GAME),
                Front::app(CASINO),
                Front::app(CLOCK),
                Front::Home,
                Front::Unknown,
                Front::LiMo,
                Front::Dialler,
                Front::EmergencyDialler,
            ] {
                for (screen_on, keyguard) in [(true, false), (true, true), (false, false), (false, true)] {
                    phone.call = call;
                    phone.front = front.clone();
                    phone.screen_on = screen_on;
                    phone.keyguard = keyguard;
                    phone.advance(1_000);
                    phone.tick();
                    let exempt = call == CallState::Ringing
                        || matches!(front, Front::LiMo | Front::Dialler | Front::EmergencyDialler)
                        || !screen_on
                        || keyguard;
                    if exempt {
                        assert_eq!(phone.orders.cover, Cover::Nothing, "{call:?} {front:?} {screen_on} {keyguard}");
                    }
                    if call == CallState::Ongoing {
                        let during = kind(&phone.orders.cover);
                        phone.call = CallState::Idle;
                        phone.tick();
                        assert_eq!(during, kind(&phone.orders.cover), "{front:?} {screen_on} {keyguard}");
                    }
                }
            }
        }
    }
}

#[test]
fn with_the_time_used_up_and_a_call_on_the_game_is_covered_and_the_dialler_is_not() {
    let mut f = locked_phone();
    let counted = f.counted();
    // Mira rings a friend and, with the call on, goes back to the game: the
    // lock covers it, and offers the way back to the call.
    f.call = CallState::Ongoing;
    f.front = Front::Dialler;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.front = Front::app(GAME);
    f.tick();
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::TimeUsedUp);
    assert!(lock.call_on);
    assert_eq!(lock.back_to_call.as_deref(), Some("Back to the call"));
    // The home screen is covered too; the dialler, the emergency dialler,
    // LiMo and the always allowed clock are not.
    f.front = Front::Home;
    f.tick();
    assert!(f.lock().call_on);
    for front in [Front::Dialler, Front::EmergencyDialler, Front::LiMo, Front::app(CLOCK)] {
        f.front = front.clone();
        f.run(2);
        assert_eq!(f.orders.cover, Cover::Nothing, "{front:?}");
    }
    // A blocked app during the call: under the lock, with the way back.
    f.front = Front::labelled(CASINO, "Lucky Spins");
    f.tick();
    let Cover::Lock(lock) = f.orders.cover.clone() else { panic!("{:?}", f.orders.cover) };
    assert!(lock.call_on);
    // Nothing was counted, and the call ends with the lock as it was.
    assert_eq!(f.counted(), counted);
    f.call = CallState::Idle;
    f.front = Front::app(GAME);
    f.tick();
    let lock = f.lock();
    assert!(!lock.call_on && lock.back_to_call.is_none());
}

#[test]
fn a_call_left_open_does_not_stop_the_count_in_a_game() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10));
        r.blocked_apps = vec![CASINO.into()];
    });
    f.run(MIN);
    assert_eq!(f.counted(), 60);
    // A call is started and left open; its own screen does not count.
    f.call = CallState::Ongoing;
    f.front = Front::Dialler;
    f.tick();
    f.run(MIN);
    assert_eq!(f.counted(), 60);
    // The game comes back to the front: it counts, the call still on.
    f.front = Front::app(GAME);
    f.tick();
    f.run(2 * MIN);
    assert!(f.orders.time_left.counting);
    assert_eq!(f.counted(), 180);
    // A blocked app is blocked during the call too, with the way back.
    f.front = Front::labelled(CASINO, "Lucky Spins");
    f.tick();
    let Cover::Block { title, call_on, back_to_call, .. } = f.orders.cover.clone() else {
        panic!("{:?}", f.orders.cover)
    };
    assert_eq!(title, "Lucky Spins is not allowed.");
    assert!(call_on);
    assert_eq!(back_to_call.as_deref(), Some("Back to the call"));
    // Back to the game: the time runs out with the call still open, warned
    // of as ever, and the lock comes on time.
    f.front = Front::app(GAME);
    f.run_until_locked();
    assert_eq!(f.counted(), 5 * 60, "half of the ten minutes, away from home");
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::AwayTimeUsedUp);
    assert!(lock.call_on);
    assert_eq!(f.warnings.iter().map(|(_, w)| w.minutes).collect::<Vec<_>>(), vec![5, 1]);
}

#[test]
fn a_blocked_app_is_covered_the_child_sent_home_and_the_guardian_told_once_a_day() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| r.blocked_apps = vec![CASINO.into()]);
    f.run(MIN);
    let counted = f.counted();
    f.front = Front::app(CASINO);
    f.tick();
    let Cover::Block { package, title, message, go_home, call_on, back_to_call } = f.orders.cover.clone() else {
        panic!("not blocked: {:?}", f.orders.cover)
    };
    assert_eq!(package, CASINO);
    assert!(go_home && !call_on && back_to_call.is_none());
    assert_eq!((title.as_str(), message.as_str()), ("This app is not allowed.", "It is blocked by your family's rules."));
    // With its label from the host, it is named, the label cleaned.
    f.front = Front::labelled(CASINO, "  Lucky\u{202E} \t Spins\n");
    f.tick();
    let Cover::Block { title, .. } = f.orders.cover.clone() else { panic!("not blocked") };
    assert_eq!(title, "Lucky Spins is not allowed.");
    assert!(!f.orders.time_left.counting);
    // Opened again and again: covered each time, never counted, noted once.
    for _ in 0..5 {
        f.run(10);
        f.front = Front::Home;
        f.run(2);
        f.front = Front::app(CASINO);
    }
    f.tick();
    assert!(matches!(f.orders.cover, Cover::Block { .. }));
    assert_eq!(f.counted(), counted);
    let day = day_of(f.now());
    let blocked: Vec<&PhoneNote> = f.notes.iter().filter(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })).collect();
    assert_eq!(blocked, vec![&PhoneNote::BlockedAppOpened { package: CASINO.into(), day, at: at(16, 1) }]);
    // The note is kept for the master, across a restart.
    f.crash();
    f.tick();
    assert!(f.view().notes.iter().any(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })));
    // The next day, it is noted again.
    f.run_until(on(1, 0, 1));
    assert_eq!(f.notes.iter().filter(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })).count(), 2);
}

#[test]
fn what_counts_is_an_app_in_front_of_an_unlocked_screen() {
    let mut f = Family::new(at(16, 0));
    f.tick();
    f.run(MIN);
    assert_eq!(f.counted(), 60);
    // The screen off, even with music playing: nothing.
    f.screen_on = false;
    f.run(MIN);
    // Android's own lock screen: nothing.
    f.screen_on = true;
    f.keyguard = true;
    f.run(MIN);
    // The home screen, LiMo, the dialler: nothing.
    f.keyguard = false;
    for front in [Front::Home, Front::LiMo, Front::Dialler, Front::EmergencyDialler] {
        f.front = front;
        f.run(MIN);
    }
    // A call ringing with a game in front: nothing.
    f.front = Front::app(GAME);
    f.call = CallState::Ringing;
    f.run(MIN);
    // The step in which the use ended counts: the host calls at once.
    assert_eq!(f.counted(), 61);
    f.call = CallState::Idle;
    // An app the host cannot name counts, with no idle rule, from the first
    // call that sees it.
    f.front = Front::Unknown;
    f.run(MIN);
    assert_eq!(f.counted(), 120);
    f.front = Front::app(GAME);
    f.run(10 * MIN);
    assert_eq!(f.counted(), 720);
    assert_eq!(f.orders.time_left.words, "48 min left");
}
