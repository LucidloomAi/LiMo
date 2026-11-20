//! The guard: a page that could switch LiMo off or remove it (the host
//! recognises it and says so in the facts) is covered until a guardian types
//! the PIN, which opens such pages for five minutes by the elapsed count.
//! Wrong PINs count as on a lock. The guard never stands where no cover may,
//! and comes after the lock and the block screen.

use limo_core::{DailyBudget, TryOutcome, WeekBudgets};
use limo_phone::{CallState, Cover, Front, GUARD_OPEN_SECS, GuardedPage, LockWhy, PhoneNote, Way};

use crate::common::*;

/// Android's Settings, as a sample name: the engine never knows which
/// package it is, the host says that a guarded page is in front.
const SETTINGS: &str = "org.example.settings";

/// Mira's phone with two hours a day and the PIN set, Settings in front with
/// nothing guarded on it yet.
fn phone_with_pin() -> Family {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    let now = f.now();
    f.hub().set_pin(PIN, now).unwrap();
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
        r.away_share_percent = 100;
    });
    f.in_reach = false;
    f.front = Front::app(SETTINGS);
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f
}

/// The notes of a guarded page given so far.
fn guarded_notes(f: &Family) -> Vec<GuardedPage> {
    f.notes
        .iter()
        .filter_map(|n| match n {
            PhoneNote::GuardedPageOpened { page, .. } => Some(*page),
            _ => None,
        })
        .collect()
}

#[test]
fn a_guarded_page_is_covered_with_the_pin_only_counts_nothing_and_is_noted_once_a_day() {
    let mut f = phone_with_pin();
    f.run(MIN);
    let counted = f.counted();
    assert_eq!(counted, 60, "Settings counts as any app does");

    // The device admin's page comes up: the guard covers it, in the engine's
    // words, with the PIN as its one way out.
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    let guard = f.guard();
    assert_eq!(guard.page, GuardedPage::DeviceAdmin);
    assert_eq!(guard.stamp, "Protected");
    assert_eq!(guard.title, "A guardian's PIN is needed here.");
    assert_eq!(guard.message, "These settings can switch LiMo off or remove it from this phone.");
    assert!(guard.can_use_pin && guard.try_wait_until.is_none() && guard.wait_words.is_none());
    assert_eq!(guard.ways.len(), 1);
    assert_eq!(guard.ways[0].way, Way::Pin);
    assert_eq!(guard.ways[0].title, "Guardian PIN");
    assert_eq!(guard.ways[0].sub, "Opens these settings for 5 minutes");
    assert!(!guard.ways[0].off);
    assert_eq!(guard.nothing_to_do, None);
    assert!(!guard.call_on && guard.back_to_call.is_none());
    // It is no lock: the time left is paused, not locked.
    assert!(!f.orders.time_left.locked && !f.orders.time_left.counting);
    assert_eq!(f.orders.time_left.words, "1 h 59 min paused");
    assert!(f.orders.save_state, "the note is kept at once");

    // What is covered is not in use: a minute under the guard counts nothing.
    f.run(MIN);
    assert_eq!(f.counted(), counted);
    assert_eq!(guarded_notes(&f), vec![GuardedPage::DeviceAdmin]);

    // Another page of Settings: nothing covered, counting again.
    f.guarded = None;
    f.run(10);
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(f.orders.time_left.counting);

    // The same page again today is not noted again; the accessibility
    // settings are, once; the next day each is noted again.
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.run(5);
    f.guarded = Some(GuardedPage::Accessibility);
    f.run(5);
    assert_eq!(f.guard().page, GuardedPage::Accessibility);
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.run(5);
    assert_eq!(guarded_notes(&f), vec![GuardedPage::DeviceAdmin, GuardedPage::Accessibility]);
    f.guarded = None;
    f.screen_on = false;
    f.run_until(on(1, 16, 0));
    f.screen_on = true;
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    assert_eq!(guarded_notes(&f), vec![GuardedPage::DeviceAdmin, GuardedPage::Accessibility, GuardedPage::DeviceAdmin]);
}

#[test]
fn the_right_pin_opens_the_guarded_pages_for_five_minutes_by_the_elapsed_count() {
    let mut f = phone_with_pin();
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    f.guard();

    // A wrong PIN changes nothing but the count of wrong tries.
    let wrong = f.way_out(|e, now| e.try_guard_pin("000000", now)).unwrap();
    assert!(matches!(wrong.outcome, TryOutcome::Wrong { wrong_guesses: 1, wait_until: None }));
    assert_eq!(wrong.message, "That PIN is not right. 4 more tries before a wait.");
    assert_eq!(wrong.opens_now, None);
    f.tick();
    f.guard();

    // The right one opens every guarded page, and says until when.
    let opened_at = f.now();
    let right = f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    assert_eq!(right.outcome, TryOutcome::ReleaseConfirmed);
    assert_eq!(right.message, "These settings are open until 4:05 pm.");
    assert_eq!(right.opens_now, None);
    assert_eq!(f.engine().guard_open_until(), Some(opened_at + GUARD_OPEN_SECS as i64));
    assert!(f.view().notes.contains(&PhoneNote::GuardOpened { until: opened_at + 300, at: opened_at }));
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(f.orders.time_left.counting, "the guardian on Settings counts, as Settings always does");
    f.guarded = Some(GuardedPage::Accessibility);
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing, "every guarded page is open");

    // Nothing else changed: no time added, the phone not unlocked.
    assert!(f.view().unsent_added.is_empty());
    assert_eq!(f.orders.time_left.unlocked_until, None);

    // A clock set forward by hand buys nothing: five minutes by the elapsed
    // count, and then guarded again.
    f.auto_time = false;
    f.skew = -2 * HOUR;
    f.run(GUARD_OPEN_SECS as i64 - 2);
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.run(2);
    assert_eq!(f.guard().page, GuardedPage::Accessibility);
    assert_eq!(f.engine().guard_open_until(), None);
    assert!(f.orders.save_state, "closing it is kept");
}

#[test]
fn the_guard_closes_when_the_phone_is_put_down_or_starts_again_and_survives_limo_being_stopped() {
    let mut f = phone_with_pin();
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    f.run(MIN);
    assert_eq!(f.orders.cover, Cover::Nothing);

    // LiMo's process is stopped and started again: still open, from what
    // was kept, and still only until the five minutes are up.
    f.crash();
    f.run(MIN);
    assert_eq!(f.orders.cover, Cover::Nothing);

    // The screen goes off: guarded again at once when it comes back on.
    f.screen_on = false;
    f.tick();
    f.advance(5_000);
    f.screen_on = true;
    f.tick();
    f.guard();

    // Android's lock screen does the same.
    f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.keyguard = true;
    f.tick();
    f.keyguard = false;
    f.tick();
    f.guard();

    // So does a restart of the phone.
    f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.reboot(30);
    f.tick();
    f.guard();
}

#[test]
fn wrong_pins_on_the_guard_count_with_the_locks_and_wait_alike() {
    let mut f = phone_with_pin();
    // Settings stays usable under a lock, so the lock and the guard meet.
    f.in_reach = true;
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(1));
        r.always_allowed_apps = vec![SETTINGS.into()];
    });
    f.in_reach = false;
    f.front = Front::app(GAME);
    f.run_until_locked();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    for _ in 0..4 {
        f.way_out(|e, now| e.try_code("1111 1111", now)).unwrap();
    }

    // Settings, allowed under the lock, with the device admin's page: the
    // guard, not the lock. The fifth wrong try, a PIN on the guard, starts
    // the wait.
    f.front = Front::app(SETTINGS);
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    assert!(f.orders.time_left.locked, "the phone is still locked under the guard");
    f.guard();
    let fifth = f.way_out(|e, now| e.try_guard_pin("000000", now)).unwrap();
    let TryOutcome::Wrong { wrong_guesses: 5, wait_until: Some(until) } = fifth.outcome else {
        panic!("not the wait: {fifth:?}")
    };
    assert_eq!(fifth.message, "Too many wrong tries. The next try is at 4:03 pm.");
    f.tick();
    let guard = f.guard();
    assert_eq!(guard.try_wait_until, Some(until));
    assert_eq!(guard.wait_words.as_deref(), Some("Too many wrong tries. The next try is at 4:03 pm."));
    assert!(guard.ways[0].off);
    assert_eq!(guard.ways[0].sub, "Wait until 4:03 pm");
    // The right PIN must wait too, and the lock's rows wait alike.
    let early = f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    assert_eq!(early.outcome, TryOutcome::MustWait { until });
    f.front = Front::app(GAME);
    f.guarded = None;
    f.tick();
    assert!(f.lock().ways.iter().filter(|w| w.way != Way::Ask).all(|w| w.off));

    // After the wait the right PIN opens the guarded pages, and only those:
    // the game is still locked.
    f.run_until(until);
    f.front = Front::app(SETTINGS);
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    assert!(!f.guard().ways[0].off);
    let right = f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    assert_eq!(right.outcome, TryOutcome::ReleaseConfirmed);
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.front = Front::app(GAME);
    f.guarded = None;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
}

#[test]
fn the_guard_never_stands_where_no_cover_may_and_comes_after_the_lock_and_the_block_screen() {
    let mut f = phone_with_pin();
    f.guarded = Some(GuardedPage::Accessibility);

    // A ringing call, Android's lock screen, a dark screen: nothing.
    f.call = CallState::Ringing;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.call = CallState::Idle;
    f.keyguard = true;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.keyguard = false;
    f.screen_on = false;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.screen_on = true;
    // LiMo's own screens and the diallers, were the host ever to say so.
    for front in [Front::LiMo, Front::Dialler, Front::EmergencyDialler] {
        f.front = front.clone();
        f.tick();
        assert_eq!(f.orders.cover, Cover::Nothing, "{front:?}");
    }
    // During a call in progress: guarded, with the way back to the call.
    f.front = Front::app(SETTINGS);
    f.call = CallState::Ongoing;
    f.tick();
    let guard = f.guard();
    assert!(guard.call_on);
    assert_eq!(guard.back_to_call.as_deref(), Some("Back to the call"));
    f.call = CallState::Idle;

    // The device admin's page only while the protection is on: with it off
    // LiMo can be removed anyway, and that page is where it is switched on.
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.permissions.protection = false;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    f.guarded = Some(GuardedPage::Accessibility);
    f.tick();
    f.guard();
    f.permissions.protection = true;

    // A blocked Settings: the block screen, not the guard.
    f.in_reach = true;
    f.set_rules(|r| r.blocked_apps = vec![SETTINGS.into()]);
    f.in_reach = false;
    f.tick();
    assert!(matches!(f.orders.cover, Cover::Block { .. }), "{:?}", f.orders.cover);
    // Locked, with Settings not allowed: the lock, not the guard.
    f.in_reach = true;
    f.set_rules(|r| {
        r.blocked_apps = Vec::new();
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(0));
    });
    f.in_reach = false;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::NoTimeToday);
}

#[test]
fn with_no_pin_set_the_guard_says_so_and_offers_nothing() {
    let mut f = Family::new(at(16, 0));
    f.front = Front::app(SETTINGS);
    f.guarded = Some(GuardedPage::DeviceAdmin);
    f.tick();
    let guard = f.guard();
    assert!(!guard.can_use_pin && guard.ways.is_empty());
    assert_eq!(
        guard.nothing_to_do,
        Some(("A guardian PIN is not set up yet.".to_string(), "A guardian sets one in LiMo on the family PC.".to_string()))
    );
    let answer = f.way_out(|e, now| e.try_guard_pin(PIN, now)).unwrap();
    assert_eq!(answer.outcome, TryOutcome::NotSet);
    assert_eq!(answer.message, "A guardian PIN is not set up yet.");
    f.tick();
    f.guard();
}

#[test]
fn nothing_is_guarded_before_setup_is_finished() {
    let mut f = Family::setting_up(at(16, 0));
    f.front = Front::app(SETTINGS);
    for page in GuardedPage::ALL {
        f.guarded = Some(page);
        f.run(5);
        assert_eq!(f.orders.cover, Cover::Nothing, "{page:?}");
    }
    assert!(guarded_notes(&f).is_empty());
    let now = f.wall();
    assert_eq!(f.engine().finish_setup(now), Ok(true));
    f.tick();
    assert_eq!(f.guard().page, GuardedPage::Accessibility);
}
