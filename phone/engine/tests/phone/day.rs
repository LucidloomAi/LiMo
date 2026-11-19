//! Whole days: the core README's worked day with the real engine on the
//! phone and the child's PC beside it on one budget; a phone that spends
//! its away time at school, is warned and locked as away, and opens again
//! at home; a plain day at home to zero; and a prod from the master.

use limo_core::{DailyBudget, WarningCause, WeekBudgets};
use limo_phone::{Cover, Front, LockWhy, Way};

use crate::common::*;

/// The worked day of the core's README: two hours, an away share of half,
/// ten minute leases. The phone is the real engine; the PC is the child's
/// device on the master PC, driven with its own device state.
#[test]
fn the_worked_day_with_the_phone_away_then_home() {
    let mut f = Family::new(at(7, 0));
    f.pair_pc();
    f.in_reach = true;
    f.screen_on = false;
    f.tick();

    // 07:30 the phone, at home, is picked up and takes a lease of ten
    // minutes; it is put down at once.
    f.run_until(at(7, 30));
    f.screen_on = true;
    f.tick();
    assert_eq!(f.engine().view().unwrap().decision.unwrap().seconds_in_hand, Some(70 * 60));
    f.screen_on = false;
    f.tick();
    // 08:00 it leaves for school without a word.
    f.run_until(at(8, 0));
    f.in_reach = false;
    f.run_until(at(10, 0));

    // 10:00 to 10:45 at school, out of reach: its lease, then 35 minutes of
    // its away allowance.
    f.screen_on = true;
    f.tick();
    f.run_until(at(10, 45));
    assert_eq!(f.counted(), 45 * 60);
    assert!(f.orders.time_left.away && f.orders.time_left.counting);
    assert_eq!(f.orders.time_left.seconds_left, Some(25 * 60));
    assert_eq!(f.orders.time_left.words, "25 min left");
    f.screen_on = false;
    f.tick();

    // 16:00 the PC: it takes leases from the fifty free minutes, then waits
    // for the rest, which the phone at school holds.
    f.run_until(at(16, 0));
    f.pc.as_mut().unwrap().using = true;
    f.run_until(at(16, 50) + 1);
    let now = f.now();
    assert!(f.pc.as_ref().unwrap().locked(now));
    assert_eq!(f.pc.as_ref().unwrap().used_today(now), 50 * 60);
    f.run_until(at(17, 0));
    assert_eq!(f.pc.as_ref().unwrap().used_today(f.now()), 50 * 60);

    // 17:00 the phone comes home and reports its 45 minutes; the PC gets
    // what the phone gives back, a little each time it runs low, until the
    // day is used up.
    f.in_reach = true;
    f.run(2);
    assert!(f.reports > 0);
    f.run_until(at(17, 40));
    let now = f.now();
    let pc = f.pc.as_ref().unwrap();
    assert!(pc.locked(now));
    assert_eq!(pc.used_today(now), 75 * 60);
    assert_eq!(f.master_used(), 120 * 60);
    assert_eq!(f.counted(), 45 * 60);

    // 19:00 the phone is picked up: it has nothing left, the PC used it at
    // home.
    f.run_until(at(19, 0));
    f.screen_on = true;
    f.tick();
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::TimeUsedUp);
    assert_eq!((lock.title.as_str(), lock.message.as_str()), ("Today's time is used up.", "You have used all of today's screen time."));
    assert_eq!(lock.until_words.as_deref(), Some("Tomorrow, 12:00 am"));
    assert_eq!(f.view().decision.unwrap().seconds_in_hand, Some(0));
}

#[test]
fn a_phone_spends_its_away_time_is_warned_as_away_and_opens_again_at_home() {
    let mut f = Family::new(at(7, 0));
    f.screen_on = false;
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(60)));
    f.run_until(at(8, 0));
    // Away all morning: half of an hour.
    f.screen_on = true;
    f.tick();
    assert_eq!(f.orders.time_left.seconds_left, Some(30 * 60));
    f.run_until_locked();
    assert_eq!(f.now(), at(8, 30));
    assert_eq!(f.counted(), 30 * 60);
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::AwayTimeUsedUp);
    assert_eq!(lock.title, "The away time is used up.");
    assert_eq!(
        lock.message,
        "This phone has used all the time it may use away from home. More comes when it is back home."
    );
    assert_eq!(lock.until_words.as_deref(), Some("Tomorrow, 12:00 am"));
    assert_eq!(lock.ways.iter().map(|w| w.way).collect::<Vec<_>>(), vec![Way::Ask, Way::Code]);
    assert_eq!(f.orders.time_left.words, "The away time is used up.");
    // Its warnings said it was the away time running out.
    let shown: Vec<(i64, u32, bool, String, String)> =
        f.warnings.iter().map(|(when, w)| (*when, w.minutes, w.away, w.message.clone(), w.hint.clone())).collect();
    assert_eq!(shown, vec![
        (
            at(8, 15),
            15,
            true,
            "15 minutes of away time left today.".to_string(),
            "More time comes when this phone is back home.".to_string()
        ),
        (at(8, 25), 5, true, "5 minutes of away time left today.".to_string(), "Save what you are doing.".to_string()),
        (at(8, 29), 1, true, "1 minute of away time left today.".to_string(), "Save what you are doing now.".to_string()),
    ]);
    assert!(f.warnings.iter().all(|(_, w)| w.cause == WarningCause::TimeRunsOut && w.when == "Locks about 8:30 am"));

    // Nothing more is counted under the lock, the screen on all the while.
    f.run_until(at(9, 0));
    assert_eq!(f.counted(), 30 * 60);

    // Back home with the lock showing: the phone reports, asks, and opens
    // in the same call.
    f.in_reach = true;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(f.orders.time_left.counting && !f.orders.time_left.away);
    // The day's other half, at home, to the end.
    f.run_until_locked();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    assert_eq!(f.counted(), 60 * 60);
    f.run(MIN);
    assert_eq!(f.master_used(), 60 * 60);
}

#[test]
fn a_plain_day_at_home_runs_to_zero_with_each_warning_once() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30)));
    f.tick();
    assert_eq!(f.orders.time_left.words, "30 min left");
    f.run_until_locked();
    assert_eq!(f.now(), at(16, 30));
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    let shown: Vec<(i64, u32, String, String)> =
        f.warnings.iter().map(|(when, w)| (*when, w.minutes, w.message.clone(), w.when.clone())).collect();
    assert_eq!(shown, vec![
        (at(16, 15), 15, "15 minutes of screen time left today.".to_string(), "Locks about 4:30 pm".to_string()),
        (at(16, 25), 5, "5 minutes of screen time left today.".to_string(), "Locks about 4:30 pm".to_string()),
        (at(16, 29), 1, "1 minute of screen time left today.".to_string(), "Locks about 4:30 pm".to_string()),
    ]);
    // The phone reported its use as it went; the master has it all.
    f.run(MIN);
    assert_eq!(f.master_used(), 30 * 60);
    assert_eq!(f.counted(), 30 * 60);
    // The home screen is covered by the lock as well, and a phone that
    // shows 24 hours is told the time that way.
    f.front = Front::Home;
    f.hours_24 = true;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::TimeUsedUp);
    assert_eq!(f.lock().until_words.as_deref(), Some("Tomorrow, 00:00"));
    // Locked until midnight with the screen in use: look again within the
    // minute (more time may come); with it off, at midnight.
    assert_eq!(f.orders.next_check_in_secs, 60);
    f.screen_on = false;
    f.tick();
    let to_midnight = u64::try_from(on(1, 0, 0) - f.now()).unwrap();
    assert!(f.orders.next_check_in_secs <= to_midnight);
}

#[test]
fn a_warning_waits_for_the_screen_and_says_the_time_actually_left() {
    let mut f = Family::new(at(20, 0));
    f.set_rules(|r| {
        r.blocked_hours =
            vec![limo_core::BlockedWindow { days: limo_core::Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }]
    });
    f.run(10 * MIN);
    // The screen goes off at 20:10, and the fifteen minute point passes
    // while nobody can read it.
    f.screen_on = false;
    f.run_until(at(20, 50));
    assert!(f.warnings.is_empty());
    // On again at 20:50: the warning comes now, and says ten minutes.
    f.screen_on = true;
    f.tick();
    let (when, w) = f.warnings[0].clone();
    assert_eq!((when, w.cause, w.minutes), (at(20, 50), WarningCause::BlockedHoursStart, 15));
    assert_eq!(w.message, "The phone locks in 10 minutes, for the hours it is not used.");
    assert_eq!((w.hint.as_str(), w.when.as_str()), ("Finish what you are doing.", "Locks at 9:00 pm"));
    // Each later point comes once, as it is reached.
    f.run_until(at(21, 0));
    let points: Vec<u32> = f.warnings.iter().map(|(_, w)| w.minutes).collect();
    assert_eq!(points, vec![15, 5, 1]);
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
}

#[test]
fn a_prod_from_the_master_brings_a_report_at_once_and_only_in_reach() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.screen_on = false;
    f.tick();
    f.run(10);
    let reports = f.reports;
    // A guardian locks Mira's devices: the master wants the phone to hear
    // of it, and the phone reports in the same call.
    let (child, now) = (f.child.clone(), f.now());
    f.hub().lock_now(&child, None, now).unwrap();
    f.advance(1_000);
    let facts = f.facts();
    assert!(facts.prodded);
    f.tick();
    assert_eq!(f.reports, reports + 1);
    f.screen_on = true;
    f.tick();
    assert_eq!(f.lock().why, LockWhy::GuardianLock);
    assert_eq!(f.lock().until_words.as_deref(), Some("When a guardian unlocks it"));
    // Out of reach, a lift waits for the phone to be back.
    f.in_reach = false;
    let now = f.now();
    f.hub().lift(&child, now).unwrap();
    f.run(MIN);
    assert_eq!(f.lock().why, LockWhy::GuardianLock);
    f.in_reach = true;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);
}

#[test]
fn a_master_that_does_not_answer_is_asked_again_after_a_wait_not_every_second() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.master_answers = false;
    f.run(30);
    // The first call reports and gets no answer; the next try waits ten
    // seconds after each one that failed.
    assert_eq!((f.attempts, f.reports), (3, 0));
    f.master_answers = true;
    f.run(11);
    assert!(f.reports >= 1);
}
