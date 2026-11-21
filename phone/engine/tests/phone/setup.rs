//! Setup: from pairing until a guardian finishes setup on the phone, nothing
//! is counted, covered or warned of, and the steps are told to the master as
//! they stand; the time left and the view say setup is not finished. Then
//! counting starts, as for any phone. Finished once, it stays finished,
//! through a save, a crash and a restart; finishing again changes nothing.

use limo_core::{BlockedWindow, DailyBudget, WeekBudgets, Weekday};
use limo_phone::{
    Cover, Front, LockWhy, Permission, PhoneEngine, PhoneError, PhoneNote, PhonePermissions, SetupView,
};

use crate::common::*;

/// Android's own settings, where the guardian spends setup.
const SETTINGS: &str = "com.android.settings";

const NOT_FINISHED: &str = "Setup is not finished on this phone.";

fn finished_notes(f: &Family) -> usize {
    f.view().notes.iter().filter(|n| matches!(n, PhoneNote::SetupFinished { .. })).count()
}

#[test]
fn a_day_of_setup_counts_nothing_and_says_so_then_counting_starts() {
    let mut f = Family::setting_up(at(16, 0));
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30));
        r.away_share_percent = 100;
    });
    assert_eq!(
        f.view().setup,
        SetupView { finished: false, finished_at: None, words: Some(NOT_FINISHED.to_string()) }
    );

    // Twenty minutes of setup: Android's own pages, LiMo's own screens, and
    // a game opened to try the phone. Nothing is counted or listed.
    f.front = Front::labelled(SETTINGS, "Settings");
    f.run(6 * MIN);
    f.front = Front::LiMo;
    f.run(2 * MIN);
    f.front = Front::app(GAME);
    f.run(12 * MIN);
    assert_eq!(f.counted(), 0);
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(!f.orders.time_left.counting);
    assert_eq!(f.orders.time_left.words, NOT_FINISHED, "the notification says so in place of the time left");
    assert_eq!(f.orders.time_left.seconds_left, Some(30 * 60), "the day's time is all still there");
    assert!(f.view().today.apps.is_empty());
    assert!(f.warnings.is_empty());

    // Permissions switched on and off while the guardian works through the
    // steps are told as they stand. Setting the rules took the phone home,
    // where its steps were told on and confirmed; of the three changes
    // since, two notes wait for the master: the last time it was on, and the
    // newest (off). Today says a guardian is needed from what the call saw.
    for accessibility in [false, true, false] {
        f.permissions = PhonePermissions { accessibility, ..PhonePermissions::ALL };
        f.run(5);
    }
    let accessibility: Vec<bool> = f
        .view()
        .notes
        .iter()
        .filter_map(|n| match n {
            PhoneNote::PermissionOn { permission: Permission::Accessibility, .. } => Some(true),
            PhoneNote::PermissionOff { permission: Permission::Accessibility, .. } => Some(false),
            _ => None,
        })
        .collect();
    assert_eq!(accessibility, [true, false]);
    let needed = f.view().today.guardian_needed.unwrap();
    assert_eq!(needed.off.iter().map(|o| o.permission).collect::<Vec<_>>(), [Permission::Accessibility]);

    // The guardian finishes setup: noted once, kept at once. The host ticks
    // at once after it, and counting starts there.
    let wall = f.wall();
    assert_eq!(f.way_out(|engine, wall| engine.finish_setup(wall)), Ok(true));
    assert_eq!(f.view().setup, SetupView { finished: true, finished_at: Some(wall), words: None });
    assert_eq!(finished_notes(&f), 1);
    assert!(f.view().notes.contains(&PhoneNote::SetupFinished { at: wall }));
    f.permissions = PhonePermissions::ALL;
    f.tick();
    f.run(2 * MIN);
    assert_eq!(f.counted(), 2 * 60);
    assert!(f.orders.time_left.counting);
    assert_eq!(f.orders.time_left.words, "28 min left");
    let apps = f.view().today.apps;
    assert_eq!(apps.iter().map(|a| a.package.as_str()).collect::<Vec<_>>(), [GAME], "setup's pages are not in Today");

    // After setup, a permission switched off is noted, and Today says a
    // guardian is needed, as for any phone.
    let from = f.notes.len();
    f.permissions = PhonePermissions { accessibility: false, ..PhonePermissions::ALL };
    f.run(5);
    let off: Vec<&PhoneNote> = f.notes[from..].iter().filter(|n| matches!(n, PhoneNote::PermissionOff { .. })).collect();
    assert!(matches!(off.as_slice(), [PhoneNote::PermissionOff { permission: Permission::Accessibility, .. }]), "{off:?}");
    let needed = f.view().today.guardian_needed.unwrap();
    assert_eq!(needed.off.iter().map(|o| o.permission).collect::<Vec<_>>(), [Permission::Accessibility]);
}

#[test]
fn a_blocked_app_and_a_lock_wait_for_setup_to_be_finished() {
    // Setup that runs into the hours screens are off: no warning, no lock,
    // no cover, nothing noted.
    let mut f = Family::setting_up(at(16, 0));
    f.set_rules(|r| {
        r.blocked_apps = vec![CASINO.into()];
        r.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 16 * 60 + 15, end_minute: 7 * 60 }];
    });
    f.front = Front::labelled(CASINO, "Lucky Spins");
    f.run(5 * MIN);
    assert_eq!(f.orders.cover, Cover::Nothing, "the guardian is holding the phone");
    f.front = Front::app(GAME);
    f.run(15 * MIN);
    assert_eq!(f.orders.cover, Cover::Nothing);
    assert!(!f.orders.time_left.locked);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
    assert!(!f.notes.iter().any(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })));
    assert_eq!(f.counted(), 0);

    // Finished: the lock is on at the first call, and LiMo itself stays open.
    assert_eq!(f.way_out(|engine, wall| engine.finish_setup(wall)), Ok(true));
    f.tick();
    assert_eq!(f.lock().why, LockWhy::BlockedHours);
    assert!(f.orders.time_left.locked);
    f.front = Front::LiMo;
    f.tick();
    assert_eq!(f.orders.cover, Cover::Nothing);

    // In the open hours, a blocked app is not covered before setup is
    // finished, and is covered after, and noted then.
    let mut g = Family::setting_up(at(10, 0));
    g.set_rules(|r| r.blocked_apps = vec![CASINO.into()]);
    g.front = Front::labelled(CASINO, "Lucky Spins");
    g.run(30);
    assert_eq!(g.orders.cover, Cover::Nothing);
    assert_eq!(g.way_out(|engine, wall| engine.finish_setup(wall)), Ok(true));
    g.tick();
    assert!(matches!(&g.orders.cover, Cover::Block { title, .. } if title == "Lucky Spins is not allowed."), "{:?}", g.orders.cover);
    assert_eq!(g.notes.iter().filter(|n| matches!(n, PhoneNote::BlockedAppOpened { .. })).count(), 1);
}

#[test]
fn finished_once_it_stays_finished_through_a_save_a_crash_and_a_restart() {
    let mut f = Family::setting_up(at(16, 0));
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30));
        r.away_share_percent = 100;
    });
    f.run(30);
    // Not finished is kept too: a crash in the middle of setup goes on with setup.
    f.crash();
    f.run(30);
    assert_eq!(f.counted(), 0);
    assert!(!f.view().setup.finished);

    let wall = f.wall();
    assert_eq!(f.way_out(|engine, wall| engine.finish_setup(wall)), Ok(true));
    let kept = f.kept.clone();
    // Finishing again changes nothing: nothing to keep, the same moment, one note.
    f.run(3);
    let before = f.kept.clone();
    assert_eq!(f.way_out(|engine, wall| engine.finish_setup(wall)), Ok(false));
    assert!(!f.engine().should_save());
    assert_eq!(f.kept, before);
    assert_eq!(f.view().setup.finished_at, Some(wall));
    assert_eq!(finished_notes(&f), 1);

    // Kept: loaded again from the bytes kept right after finishing, it is finished.
    let again = PhoneEngine::load(&kept);
    assert_eq!(again.view().unwrap().setup, SetupView { finished: true, finished_at: Some(wall), words: None });
    // Killed after three calls of a game (two seconds counted from the
    // first), and back within a second: nothing lost, counting goes on.
    f.crash();
    f.tick();
    assert_eq!(f.counted(), 2);
    f.run(60);
    assert_eq!(f.counted(), 62);
    // The phone restarted: still finished, counting from the first call.
    f.reboot(5 * MIN);
    f.tick();
    let base = f.counted();
    assert!(base >= 62 - 5, "at most the seconds since the last save are lost: {base}");
    f.run(60);
    assert!(f.view().setup.finished);
    assert_eq!(f.counted(), base + 60);
    assert_eq!(finished_notes(&f), 1);
}

#[test]
fn a_state_kept_before_setup_was_kept_counts_as_finished() {
    // A phone set up and in use, kept by a LiMo that did not know of setup:
    // its state has no word of it. It keeps its limits.
    let mut f = Family::setting_up(at(16, 0));
    let text = String::from_utf8(f.kept.clone()).unwrap();
    let body = text.splitn(4, '\n').nth(3).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["setup"], serde_json::json!("not_finished"), "a new pairing keeps it");
    value.as_object_mut().unwrap().remove("setup");
    let body = serde_json::to_string(&value).unwrap();
    let blob = format!("limo-phone-state\nformat 1\ncheck {:016x}\n{body}", fnv(body.as_bytes()));
    let mut engine = PhoneEngine::load(blob.as_bytes());
    assert_eq!(engine.load_problem(), None);
    assert_eq!(engine.view().unwrap().setup, SetupView { finished: true, finished_at: None, words: None });
    let orders = engine.tick(&f.facts());
    assert_ne!(orders.time_left.words, NOT_FINISHED);
    assert_eq!(engine.finish_setup(f.wall()), Ok(false), "already finished: nothing changes");

    // The new form keeps it in so many words.
    let wall = f.wall();
    f.engine().finish_setup(wall).unwrap();
    let kept = String::from_utf8(f.engine().save()).unwrap();
    assert!(kept.contains(&format!("\"setup\":{{\"finished\":{{\"at\":{wall}}}}}")), "{kept}");
}

#[test]
fn a_state_that_cannot_be_read_cannot_be_finished() {
    let mut engine = PhoneEngine::load(b"not LiMo's");
    assert_eq!(engine.finish_setup(at(16, 0)), Err(PhoneError::CannotCheck));
    assert!(!engine.should_save());
    assert!(engine.view().is_none());
}

/// FNV-1a, 64 bits, as the kept state's check.
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
