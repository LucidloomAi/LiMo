//! The child's own screens: Today through a day (the time left, the time
//! used, where it went, the hours screens are off, each app's minutes, kept
//! across a crash and starting again the next day); a day with no limit and
//! a guardian's unlock; the rules in words for How it works; and a guardian
//! needed while anything LiMo needs is switched off, which changes nothing
//! counted or covered.

use limo_core::{BlockedWindow, DailyBudget, PinAction, WeekBudgets, Weekday};
use limo_phone::{Front, LeftKind, Permission, PhoneNote, PhonePermissions, TodayApp};

use crate::common::*;

/// A maps app: an ordinary app that counts.
const MAPS: &str = "org.example.maps";
/// An app used for less than a minute.
const BRIEF: &str = "org.example.brief";

fn bedtime(days: &[Weekday], from: u16, to: u16) -> BlockedWindow {
    BlockedWindow { days: days.to_vec(), start_minute: from * 60, end_minute: to * 60 }
}

fn app(package: &str, secs: u64, minutes: &str) -> TodayApp {
    TodayApp { package: package.into(), secs, minutes: minutes.into() }
}

#[test]
fn today_says_the_time_left_the_time_used_and_each_apps_minutes_and_starts_again_the_next_day() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
        r.away_share_percent = 100;
        r.blocked_hours = vec![bedtime(&Weekday::ALL, 21, 7)];
        r.always_allowed_apps = vec![CLOCK.into()];
    });
    // Out of reach, with the whole day in hand. Each step belongs to the app
    // in front at its start, so the first second after a change is the
    // app before's.
    f.run(12 * MIN);
    f.front = Front::app(MAPS);
    f.run(9 * MIN);
    f.front = Front::labelled(CLOCK, "Clock");
    f.run(4 * MIN);
    f.front = Front::app(BRIEF);
    f.run(40);
    f.front = Front::Unknown;
    f.run(MIN);
    // LiMo itself, in the same second: nothing more is counted.
    f.front = Front::LiMo;
    f.tick();
    // The clock is recorded and does not count; the app with no name counts.
    assert_eq!(f.counted(), 721 + 540 + 40 + 59);

    let today = f.view().today;
    assert_eq!((today.left.kind, today.left.words.as_str()), (LeftKind::Left, "1 h 38 min"));
    assert_eq!(today.used, "22 min used of 2 h");
    assert_eq!(today.used_part, Some(0.189));
    // The phone knows its own use only.
    assert_eq!(today.where_used, "On this phone 22 min");
    assert_eq!(today.bedtime.as_deref(), Some("Screens are off from 9:00 pm to 7:00 am."));
    // Longest first; under a minute, and the app with no name, left out;
    // packages only, never a label.
    assert_eq!(today.apps, vec![app(GAME, 721, "12 min"), app(MAPS, 540, "9 min"), app(CLOCK, 240, "4 min")]);
    assert_eq!(today.guardian_needed, None);
    assert!(!serde_json::to_string(&today).unwrap().contains("Clock\""));

    // Kept across a crash, with the state.
    f.screen_on = false;
    f.tick();
    let before = f.view().today;
    f.crash();
    f.advance(1_000);
    f.tick();
    assert_eq!(f.view().today, before);

    // At bedtime the phone is locked; LiMo itself still shows Today.
    f.advance((at(21, 30) - f.now()) as u64 * 1_000);
    f.screen_on = true;
    f.tick();
    assert_eq!(f.view().today.bedtime.as_deref(), Some("Screens are off from 9:00 pm to 7:00 am."));
    assert_eq!(f.view().today.left.words, "1 h 38 min");

    // The next morning the count starts again.
    f.screen_on = false;
    f.advance((on(1, 9, 0) - f.now()) as u64 * 1_000);
    f.tick();
    let morning = f.view().today;
    assert_eq!(morning.apps, Vec::new());
    assert_eq!((morning.left.words.as_str(), morning.used.as_str()), ("2 h", "0 min used of 2 h"));
    assert_eq!((morning.used_part, morning.where_used.as_str()), (Some(0.0), "On this phone 0 min"));
    f.screen_on = true;
    f.front = Front::app(GAME);
    f.tick();
    f.run(2 * MIN);
    assert_eq!(f.view().today.apps, vec![app(GAME, 120, "2 min")]);
    assert_eq!(f.view().today.used, "2 min used of 2 h");
}

#[test]
fn a_day_with_no_limit_and_a_guardians_unlock_in_todays_words() {
    let mut f = Family::new(at(20, 0));
    f.in_reach = true;
    let now = f.now();
    f.hub().set_pin(PIN, now).unwrap();
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::NoLimit));
    f.in_reach = false;
    f.run(3 * MIN);
    let today = f.view().today;
    assert_eq!((today.left.kind, today.left.words.as_str()), (LeftKind::NoLimit, "No limit today"));
    assert_eq!((today.used.as_str(), today.used_part), ("3 min used today", None));

    // Ten minutes a day, used up; a guardian unlocks it for half an hour.
    f.in_reach = true;
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10));
        r.away_share_percent = 100;
    });
    f.in_reach = false;
    f.run_until_locked();
    let until = f.wall() + 30 * MIN;
    f.way_out(|e, now| e.try_pin(PIN, PinAction::UnlockUntil { until }, now)).unwrap();
    f.run(5 * MIN);
    let today = f.view().today;
    assert_eq!(today.left.kind, LeftKind::Open);
    // Said rounded up to the minute, as every time it opens.
    assert_eq!(today.left.words, format!("Open until 8:{:02} pm", (until - at(20, 0) + MIN - 1) / MIN));
    // Use under the unlock is counted beyond the day's time: the 3 minutes
    // of the day with no limit, the rest of the ten, and nearly five more.
    assert_eq!(f.counted(), 15 * MIN as u64 - 1);
    assert_eq!(today.used, "14 min used of 10 min");
    assert_eq!(today.used_part, Some(1.0));
}

#[test]
fn how_it_works_says_the_rules_in_words_on_the_phones_clock() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(90));
        r.budgets.saturday = DailyBudget::Minutes(180);
        r.budgets.sunday = DailyBudget::Minutes(180);
        let school_nights = [Weekday::Sunday, Weekday::Monday, Weekday::Tuesday, Weekday::Wednesday, Weekday::Thursday];
        r.blocked_hours = vec![bedtime(&school_nights, 21, 7), bedtime(&[Weekday::Friday, Weekday::Saturday], 22, 8)];
        r.always_allowed_apps = vec![CLOCK.into(), MAPS.into()];
        r.blocked_apps = vec![CASINO.into()];
    });
    let rules = f.view().rules_words;
    assert_eq!(rules.time_a_day, "1 h 30 min from Monday to Friday; 3 h on Saturday and Sunday");
    assert_eq!(
        rules.screens_off.as_deref(),
        Some("9:00 pm to 7:00 am from Sunday to Thursday; 10:00 pm to 8:00 am on Friday and Saturday")
    );
    assert_eq!(rules.always_open, vec![CLOCK.to_string(), MAPS.to_string()]);
    assert_eq!(rules.not_allowed, vec![CASINO.to_string()]);
    // A Wednesday, so a school night, on a phone that shows 24 hours.
    f.hours_24 = true;
    f.tick();
    assert_eq!(f.view().today.bedtime.as_deref(), Some("Screens are off from 21:00 to 07:00."));
    assert_eq!(
        f.view().rules_words.screens_off.as_deref(),
        Some("21:00 to 07:00 from Sunday to Thursday; 22:00 to 08:00 on Friday and Saturday")
    );
    // No blocked hours, and the same time every day.
    f.set_rules(|r| {
        r.blocked_hours.clear();
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
    });
    let rules = f.view().rules_words;
    assert_eq!((rules.time_a_day.as_str(), rules.screens_off), ("2 h", None));
    assert_eq!(f.view().today.bedtime, None);
}

/// One second on two phones that differ only in their permissions: the
/// same cover, the same time left, the same warning.
fn step_both(on: &mut Family, off: &mut Family) {
    on.advance(1_000);
    off.advance(1_000);
    let a = on.tick().clone();
    let b = off.tick().clone();
    assert_eq!((&a.cover, &a.time_left, &a.warning), (&b.cover, &b.time_left, &b.warning), "call {}", on.calls);
}

#[test]
fn a_guardian_is_needed_while_anything_is_switched_off_and_nothing_counted_or_covered_changes() {
    let mut on = Family::new(at(16, 0));
    let mut off = Family::new(at(16, 0));
    for f in [&mut on, &mut off] {
        f.set_rules(|r| {
            r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10));
            r.away_share_percent = 100;
            r.blocked_apps = vec![CASINO.into()];
        });
    }
    assert_eq!(off.view().today.guardian_needed, None);

    // Every permission switched off at once: noted in setup order, and Today
    // sends for a guardian with one sentence for each.
    off.permissions = PhonePermissions {
        notifications: false,
        phone: false,
        usage_access: false,
        accessibility: false,
        battery: false,
        protection: false,
    };
    step_both(&mut on, &mut off);
    let noted: Vec<Permission> = off
        .notes
        .iter()
        .filter_map(|n| match n {
            PhoneNote::PermissionOff { permission, .. } => Some(*permission),
            _ => None,
        })
        .collect();
    assert_eq!(noted, Permission::ALL.to_vec());
    let needed = off.view().today.guardian_needed.unwrap();
    let said: Vec<(Permission, &str)> = needed.off.iter().map(|o| (o.permission, o.words.as_str())).collect();
    assert_eq!(said, vec![
        (Permission::Notifications, "Notifications are switched off, so LiMo cannot show the time left or a warning."),
        (Permission::Phone, "Phone access is switched off, so a lock could hide a call."),
        (Permission::UsageAccess, "Usage access is switched off, so LiMo cannot count the time in each app."),
        (Permission::Accessibility, "Accessibility is switched off, so LiMo cannot see which app is open."),
        (Permission::Battery, "Android may put LiMo to sleep to save battery."),
        (Permission::Protection, "Protection is switched off, so LiMo can be removed."),
    ]);
    assert_eq!(on.view().today.guardian_needed, None);

    // Through a game, a blocked app, the lock, the home screen, a call and
    // the dialler, both phones are told the same at every call.
    let script: [(Front, limo_phone::CallState, i64); 6] = [
        (Front::app(GAME), limo_phone::CallState::Idle, 4 * MIN),
        (Front::app(CASINO), limo_phone::CallState::Idle, 10),
        (Front::app(GAME), limo_phone::CallState::Ongoing, 7 * MIN),
        (Front::Home, limo_phone::CallState::Idle, 10),
        (Front::app(GAME), limo_phone::CallState::Ringing, 10),
        (Front::Dialler, limo_phone::CallState::Ongoing, 10),
    ];
    for (front, call, secs) in script {
        for f in [&mut on, &mut off] {
            f.front = front.clone();
            f.call = call;
        }
        for _ in 0..secs {
            step_both(&mut on, &mut off);
        }
    }
    assert_eq!(on.counted(), 10 * MIN as u64);
    assert_eq!(off.counted(), on.counted());

    // Only the phone permission off: one sentence. All on again: nothing.
    off.permissions = PhonePermissions { phone: false, ..PhonePermissions::ALL };
    step_both(&mut on, &mut off);
    let needed = off.view().today.guardian_needed.unwrap();
    assert_eq!(needed.off.len(), 1);
    assert_eq!(needed.off[0].permission, Permission::Phone);
    off.permissions = PhonePermissions::ALL;
    step_both(&mut on, &mut off);
    assert_eq!(off.view().today.guardian_needed, None);
    off.permissions = PhonePermissions { phone: false, ..PhonePermissions::ALL };
    step_both(&mut on, &mut off);
    let phone_offs = off.notes.iter().filter(|n| matches!(n, PhoneNote::PermissionOff { permission: Permission::Phone, .. })).count();
    assert_eq!(phone_offs, 2, "noted each time it is switched off");
}
