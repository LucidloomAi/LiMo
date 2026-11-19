//! The clock: the wall clock set by hand (not followed, so it buys nothing)
//! and set by the network (followed); a time zone change; the phone
//! restarted in the middle of a step, and LiMo's process restarted in the
//! middle of one; LiMo held up while the phone is in use. Counted time comes
//! from the elapsed count alone; the guardian is told of each.

use limo_core::{BlockedWindow, Weekday};
use limo_phone::{LockWhy, PhoneNote};

use crate::common::*;

fn clock_notes(f: &Family) -> Vec<PhoneNote> {
    f.notes
        .iter()
        .filter(|n| matches!(n, PhoneNote::ClockMoved { .. } | PhoneNote::TimeZoneChanged { .. } | PhoneNote::Restarted { .. }))
        .cloned()
        .collect()
}

#[test]
fn a_clock_set_by_hand_is_not_followed_and_buys_nothing() {
    let mut f = Family::new(at(20, 0));
    f.auto_time = false;
    f.set_rules(|r| {
        r.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }]
    });
    f.run(10 * MIN);
    assert_eq!(f.counted(), 600);
    // Back two hours, to dodge bedtime: ten minutes of use count as ten,
    // and bedtime still comes at 21:00, by the true time.
    f.skew -= 2 * HOUR;
    f.run(10 * MIN);
    assert_eq!(f.counted(), 1_200);
    assert_eq!(f.view().clock_correction_secs, -2 * HOUR);
    // Forward a day, for a new day's time: the day does not change.
    f.skew += 26 * HOUR;
    f.run(10 * MIN);
    assert_eq!(f.counted(), 1_800);
    assert_eq!(f.view().day, day_of(at(12, 0)));
    f.run_until(at(21, 0));
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
    assert_eq!(f.lock().until_words.as_deref(), Some("Tomorrow, 7:00 am"));
    let blocked_warnings: Vec<String> = f.warnings.iter().map(|(_, w)| w.message.clone()).collect();
    assert_eq!(blocked_warnings, vec![
        "The phone locks in 15 minutes, for the hours it is not used.".to_string(),
        "The phone locks in 5 minutes, for the hours it is not used.".to_string(),
        "The phone locks in 1 minute, for the hours it is not used.".to_string(),
    ]);
    assert_eq!(clock_notes(&f), vec![
        PhoneNote::ClockMoved { by_secs: -2 * HOUR, followed: false, at: at(20, 10) + 1 },
        PhoneNote::ClockMoved { by_secs: 26 * HOUR, followed: false, at: at(20, 20) + 1 },
    ]);
    // Through a crash, LiMo keeps its own clock.
    f.crash();
    f.run(MIN);
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
    assert_eq!(f.view().clock_correction_secs, 24 * HOUR);
    // Automatic time switched back on: the network's clock is right again,
    // and LiMo follows it.
    f.skew = 0;
    f.auto_time = true;
    f.run(MIN);
    assert_eq!(f.view().clock_correction_secs, 0);
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
    f.run(MIN);
    assert_eq!(f.counted(), 3_600);
}

#[test]
fn a_clock_set_by_the_network_is_followed_and_counts_the_same() {
    let mut f = Family::new(at(14, 0));
    f.tick();
    f.run(10 * MIN);
    // The network corrects a clock that was three minutes slow.
    f.skew += 3 * MIN;
    f.run(10 * MIN);
    assert_eq!(f.counted(), 1_200);
    assert_eq!(f.view().clock_correction_secs, 0);
    assert_eq!(clock_notes(&f), vec![PhoneNote::ClockMoved { by_secs: 180, followed: true, at: at(14, 10) + 1 + 180 }]);
    // The time left followed the use only.
    assert_eq!(f.orders.time_left.seconds_left, Some(3_600 - 1_200));
}

#[test]
fn a_time_zone_change_is_noted_and_the_home_zone_still_rules() {
    let mut f = Family::new(at(20, 0));
    f.set_rules(|r| {
        r.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }]
    });
    f.run(MIN);
    // Off on a trip: the phone's zone is now UTC. Bedtime is still 21:00 at
    // home, and the lock says when it opens in the phone's own time.
    f.offset = 0;
    f.run(MIN);
    assert_eq!(clock_notes(&f), vec![PhoneNote::TimeZoneChanged {
        from_offset_secs: IST,
        to_offset_secs: 0,
        at: at(20, 1) + 1
    }]);
    f.run_until(at(21, 0));
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::BlockedHours);
    assert_eq!(lock.until_words.as_deref(), Some("Tomorrow, 1:30 am"));
    assert_eq!(f.counted(), 3_600);
}

#[test]
fn a_restart_of_the_phone_in_the_middle_of_a_step_counts_nothing_for_the_step() {
    let mut f = Family::new(at(14, 0));
    f.tick();
    f.run(10 * MIN);
    assert_eq!(f.counted(), 600);
    // The phone is turned off half a second after a call and on again
    // twenty seconds later: the step across counts nothing, and the
    // restart is noted. What was counted since the state was last kept, at
    // most five seconds, is lost with the process.
    f.advance(500);
    f.reboot(20);
    f.tick();
    let kept = f.counted();
    assert!((595..=600).contains(&kept), "{kept}");
    f.run(MIN);
    assert_eq!(f.counted(), kept + 60);
    assert_eq!(clock_notes(&f), vec![PhoneNote::Restarted { at: at(14, 10) + 20 }]);
    // Set back by hand while it was off, with automatic time off: LiMo's
    // clock does not go back.
    f.auto_time = false;
    f.run(MIN);
    let before = f.counted();
    f.skew = -HOUR;
    f.reboot(30);
    f.tick();
    let notes = clock_notes(&f);
    assert!(matches!(notes.last(), Some(PhoneNote::ClockMoved { followed: false, .. })), "{notes:?}");
    assert!(before - f.counted() <= 5);
    // LiMo's clock goes on from where it last kept it, at most five seconds
    // before the phone went off: the half minute it was off is lost to it,
    // the hour set back is not.
    let correction = f.view().clock_correction_secs;
    assert!((-HOUR + 30 - 5..=-HOUR + 30).contains(&correction), "{correction}");
}

#[test]
fn limos_process_restarted_in_the_middle_of_a_step_counts_the_step_once() {
    let mut f = Family::new(at(14, 0));
    f.tick();
    f.run(7);
    // Killed four seconds after it last kept the state, and back three
    // seconds later: the seven seconds since are one step, counted once.
    f.advance(3_000);
    f.crash();
    f.tick();
    f.run(10 * MIN);
    assert_eq!(f.counted(), 600 + 10);
    // Killed and back only after a minute while the phone was in use: not
    // counted, and noted.
    f.advance(70_000);
    f.crash();
    f.tick();
    let counted = f.counted();
    assert!((605..=610).contains(&counted), "{counted}");
    assert!(f.notes.iter().any(|n| matches!(n, PhoneNote::NotWatched { secs: 70..=75, .. })), "{:?}", f.notes);
}

#[test]
fn a_permission_switched_off_is_noted() {
    let mut f = Family::new(at(14, 0));
    f.run(5);
    f.permissions.accessibility = false;
    f.front = limo_phone::Front::Unknown;
    f.run(5);
    f.permissions.usage_access = false;
    f.run(5);
    let off: Vec<&PhoneNote> = f.notes.iter().filter(|n| matches!(n, PhoneNote::PermissionOff { .. })).collect();
    assert_eq!(off.len(), 2);
    // With nothing known of the app in front, the time still counts.
    assert_eq!(f.counted(), 14);
}
