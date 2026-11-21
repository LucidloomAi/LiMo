//! The phone's notes and the use on the child's other devices: notes kept
//! away from home and carried to the master in every report until a grant
//! confirms them, then dropped, the kinds the master does not take staying
//! on the phone; a new note bringing a report at once in reach, and no
//! report after report from a master that never confirms; Today saying how
//! much went on the PC once the master says it, away too, and only this
//! phone's use until then, the next day, and on a phone that never reaches
//! its master; and a state kept before notes went anywhere, which loads and
//! sends them.

use limo_core::{DailyBudget, DeviceKind, GrantOutcome, MasterBook, RuleSet, SentNote, WeekBudgets};
use limo_phone::{Front, Permission, PhoneEngine, PhoneFacts, PhoneNote, PhonePermissions};

use crate::common::*;

/// The ids of the notes a report carried, and their kinds.
fn carried(report: &limo_core::Report) -> Vec<(String, limo_core::PhoneNote)> {
    report.notes.iter().map(|n| (n.id.clone(), n.note.clone())).collect()
}

fn is_note_for_master(note: &PhoneNote) -> bool {
    note.for_master().is_some()
}

/// A note the master takes, by its kind alone.
fn kind(note: &limo_core::PhoneNote) -> String {
    serde_json::to_value(note).unwrap()["note"].as_str().unwrap().to_string()
}

#[test]
fn notes_go_to_the_master_in_every_report_until_confirmed_and_the_others_stay() {
    let mut f = Family::new(at(16, 0));
    let finished_at = f.wall();
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
        r.away_share_percent = 100;
        r.blocked_apps = vec![CASINO.into()];
    });
    // Setting the rules took the phone home once: its setup finished went,
    // the first note it ever sent, then how it stands (six steps on, base
    // protection), and all were confirmed.
    let first = f.sent.first().unwrap().clone();
    let phone = f.phone_id.clone();
    assert_eq!(carried(&first)[0], (format!("{phone}/note/1"), limo_core::PhoneNote::SetupFinished { at: finished_at }));
    let kinds: Vec<String> = carried(&first).iter().map(|(_, n)| kind(n)).collect();
    assert_eq!(kinds, [
        "setup_finished", "permission_on", "permission_on", "permission_on", "permission_on", "permission_on",
        "permission_on", "protection",
    ]);
    assert!(f.view().notes.is_empty());

    // Away: accessibility switched off a moment (and with it the guard, so
    // the protection level), a blocked app opened, the phone restarted.
    // Kept, nothing sent.
    let sent_before = f.sent.len();
    f.run(MIN);
    f.permissions = PhonePermissions { accessibility: false, ..PhonePermissions::ALL };
    f.run(2);
    f.permissions = PhonePermissions::ALL;
    f.front = Front::app(CASINO);
    f.run(2);
    f.front = Front::app(GAME);
    f.reboot(90);
    f.run(MIN);
    assert_eq!(f.sent.len(), sent_before);
    let kinds: Vec<&str> = f
        .view()
        .notes
        .iter()
        .map(|n| match n {
            PhoneNote::PermissionOff { permission: Permission::Accessibility, .. } => "off",
            PhoneNote::PermissionOn { permission: Permission::Accessibility, .. } => "on",
            PhoneNote::Protection { .. } => "protection",
            PhoneNote::BlockedAppOpened { .. } => "blocked",
            PhoneNote::Restarted { .. } => "restarted",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(kinds, ["off", "protection", "on", "protection", "blocked", "restarted"]);

    // Home, and the master does not answer: the report carries the five the
    // master takes, numbered on from the first; they stay.
    f.in_reach = true;
    f.master_answers = false;
    f.run(2);
    let unanswered = f.sent.last().unwrap().clone();
    let ids: Vec<String> = carried(&unanswered).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, (9..=13).map(|n| format!("{phone}/note/{n}")).collect::<Vec<_>>());
    assert!(matches!(carried(&unanswered)[0].1, limo_core::PhoneNote::PermissionOff { permission: limo_core::PhonePermission::Accessibility, .. }));
    assert_eq!(f.view().notes.len(), 6);
    // It answers: the same ids again, confirmed, and dropped; the blocked
    // app, a kind the master does not take, stays on the phone.
    f.master_answers = true;
    f.run(12);
    let answered = f.sent.last().unwrap().clone();
    assert!(f.sent.len() > sent_before + 1);
    let again: Vec<String> = f.sent[sent_before + 1].notes.iter().map(|n| n.id.clone()).collect();
    assert_eq!(again, ids, "the same notes, by the same ids");
    assert!(answered.notes.is_empty() || answered.notes.iter().all(|n| ids.contains(&n.id)));
    assert_eq!(f.view().notes.len(), 1);
    assert!(matches!(f.view().notes[0], PhoneNote::BlockedAppOpened { .. }));
    // Later reports carry none, and a crash brings none back.
    f.crash();
    f.run(MIN);
    assert!(f.sent.last().unwrap().notes.is_empty());
    assert_eq!(f.view().notes.len(), 1);
}

#[test]
fn a_new_note_in_reach_brings_a_report_at_once_and_a_master_that_never_confirms_no_more() {
    let mut f = Family::new(at(16, 0));
    f.in_reach = true;
    f.screen_on = false;
    f.run(20);
    // Nothing is due: the phone is idle and its last exchange was just now.
    let quiet = f.sent.len();
    f.run(30);
    assert_eq!(f.sent.len(), quiet);
    // A permission switched off: noted in one call, sent in the next.
    f.permissions = PhonePermissions { battery: false, ..PhonePermissions::ALL };
    f.tick();
    assert_eq!(f.sent.len(), quiet);
    f.run(1);
    assert_eq!(f.sent.len(), quiet + 1);
    assert!(matches!(f.sent[quiet].notes.as_slice(), [SentNote { note: limo_core::PhoneNote::PermissionOff { .. }, .. }]));
    assert!(f.view().notes.is_empty());

    // A master from before notes confirms none: the notes (the battery on
    // again, notifications off) ride along with the reports that come
    // anyway, and bring no report of their own after the first.
    f.old_master = true;
    f.permissions = PhonePermissions::ALL;
    f.run(1);
    f.permissions = PhonePermissions { notifications: false, ..PhonePermissions::ALL };
    f.run(2);
    let after_note = f.sent.len();
    assert_eq!(f.sent.last().unwrap().notes.len(), 2);
    f.run(2 * MIN);
    assert_eq!(f.sent.len(), after_note, "no report after report");
    assert_eq!(f.view().notes.len(), 2, "kept, as nothing confirmed them");
    f.run(5 * MIN);
    assert!(f.sent.len() > after_note, "the idle report comes");
    assert_eq!(f.sent.last().unwrap().notes.len(), 2);
}

#[test]
fn setup_finished_in_reach_reaches_the_master_at_the_next_call() {
    let mut f = Family::setting_up(at(16, 0));
    f.in_reach = true;
    f.run(20);
    let before = f.sent.len();
    let wall = f.wall();
    assert_eq!(f.way_out(|e, _| e.finish_setup(wall)), Ok(true));
    f.run(1);
    assert_eq!(f.sent.len(), before + 1);
    // With it, the protection it brings: the guard covers from now on.
    let carried: Vec<limo_core::PhoneNote> = f.sent[before].notes.iter().map(|n| n.note.clone()).collect();
    assert!(
        matches!(carried.as_slice(), [
            limo_core::PhoneNote::SetupFinished { at },
            limo_core::PhoneNote::Protection { level: limo_core::ProtectionLevel::Base, .. },
        ] if *at == wall),
        "{carried:?}"
    );
}

#[test]
fn today_says_how_much_went_on_the_pc_once_the_master_says_it() {
    let mut f = Family::new(at(15, 0));
    f.pair_pc();
    f.set_rules(|r| {
        r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
        r.away_share_percent = 50;
    });
    // A phone that never reaches its master, in a family whose PC is used
    // too: the same words as before the master could say more.
    let mut never = Family::new(at(15, 0));
    never.pair_pc();
    never.screen_on = false;
    never.pc.as_mut().unwrap().using = true;
    never.run(17 * MIN);
    never.pc.as_mut().unwrap().using = false;
    never.screen_on = true;

    // 17 minutes on the PC, the phone's screen off, at home.
    f.in_reach = true;
    f.screen_on = false;
    f.pc.as_mut().unwrap().using = true;
    f.run(17 * MIN);
    f.pc.as_mut().unwrap().using = false;
    f.run(2 * MIN);
    let now = f.now();
    let pc_used = f.pc.as_ref().unwrap().used_today(now);
    assert!((16 * 60..=17 * 60).contains(&pc_used), "{pc_used}");
    // Then the phone.
    f.screen_on = true;
    f.run(31 * MIN);
    f.screen_on = false;
    f.run(2 * MIN);
    let phone_used = f.counted();
    let (pc_min, phone_min) = (pc_used / 60, phone_used / 60);
    let today = f.view().today;
    assert_eq!(today.where_used, format!("On this phone {phone_min} min \u{b7} on the PC {pc_min} min"));
    assert_eq!(today.used, format!("{} min used of 2 h", (pc_used + phone_used) / 60));
    assert_eq!(today.used_part, Some(((pc_used + phone_used) as f64 / 7_200.0 * 1_000.0).round() / 1_000.0));

    // Away, the figure stays what the master last said.
    f.in_reach = false;
    f.screen_on = true;
    f.run(MIN);
    assert!(f.view().today.where_used.ends_with(&format!("on the PC {pc_min} min")));
    f.crash();
    f.run(1);
    assert!(f.view().today.where_used.ends_with(&format!("on the PC {pc_min} min")), "kept with the state");

    // The next morning, at home: nothing used anywhere yet.
    f.screen_on = false;
    f.advance((on(1, 8, 0) - f.now()) as u64 * 1_000);
    f.in_reach = true;
    f.run(5);
    let morning = f.view().today;
    assert_eq!((morning.where_used.as_str(), morning.used.as_str()), ("On this phone 0 min", "0 min used of 2 h"));

    // The phone that never reached its master knows only its own use.
    never.run(10 * MIN);
    let today = never.view().today;
    assert!(never.pc.as_ref().unwrap().used_today(never.now()) > 15 * 60);
    let mine = never.counted() / 60;
    assert!(mine >= 9);
    assert_eq!(today.where_used, format!("On this phone {mine} min"));
    assert_eq!(today.used, format!("{mine} min used of 2 h"));
    assert!(never.sent.is_empty());
}

#[test]
fn use_on_more_than_the_pc_is_said_as_other_devices_and_an_old_master_says_nothing() {
    let mut f = Family::new(at(15, 0));
    f.pair_pc();
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120)));
    f.in_reach = true;
    f.screen_on = false;
    f.pc.as_mut().unwrap().using = true;
    f.run(20 * MIN);
    f.pc.as_mut().unwrap().using = false;
    f.screen_on = true;
    f.run(5 * MIN);
    assert!(f.view().today.where_used.contains("on the PC"));

    // A grant saying some of the rest was on another device.
    let wall = f.wall();
    let now = f.now();
    let report = f.engine().outgoing_report(wall).unwrap();
    let mut grant = f.hub().handle_report(&report, now).unwrap();
    grant.others_all_on_master = false;
    assert_eq!(f.engine().take_grant(&grant, wall).outcome, GrantOutcome::Taken);
    f.tick();
    let today = f.view().today;
    assert!(today.where_used.starts_with("On this phone "), "{}", today.where_used);
    assert!(today.where_used.contains(" \u{b7} on other devices "), "{}", today.where_used);

    // A master from before the figure: the phone says its own use only,
    // from its next report on.
    f.old_master = true;
    let sent = f.sent.len();
    f.run(MIN + 5);
    assert!(f.sent.len() > sent);
    let today = f.view().today;
    assert_eq!(today.where_used, format!("On this phone {} min", f.counted() / 60));
    assert_eq!(today.used, format!("{} min used of 2 h", f.counted() / 60));
}

#[test]
fn a_state_kept_before_notes_went_anywhere_loads_and_sends_them() {
    // Kept by the engine before notes went to the master: a phone paired
    // and set up, a game for two minutes, accessibility switched off, a
    // blocked app, a restart and the network moving the clock by an hour,
    // all away from home.
    let bytes = include_bytes!("fixtures/kept_before_notes_travelled.limo");
    let mut engine = PhoneEngine::load(bytes);
    assert_eq!(engine.load_problem(), None);
    let view = engine.view().unwrap();
    assert_eq!(view.notes.len(), 5);
    assert_eq!(view.notes.iter().filter(|n| is_note_for_master(n)).count(), 4);
    assert!(view.today.where_used.starts_with("On this phone ") && !view.today.where_used.contains('\u{b7}'));

    // Its master, as it was: the same child, rules and phone.
    let start = RuleSet::new(1, IST).calendar().at(limo_core::CivilDate::new(2026, 11, 18).unwrap(), 7, 0, 0);
    let mut rules = RuleSet::new(1, IST);
    rules.budgets = WeekBudgets::every_day(DailyBudget::Minutes(120));
    rules.blocked_apps = vec![CASINO.into()];
    let mut book = MasterBook::new("asha", rules).unwrap();
    book.add_device("phone", DeviceKind::Portable, start).unwrap();

    // Home ten seconds after it was kept: the four it keeps for the master
    // go, numbered from 1, and after them how the phone stands, which that
    // engine never told: every step on, and base protection.
    let now = start + 3_913;
    let facts = PhoneFacts {
        wall_secs: now,
        elapsed_ms: 101_000,
        boot_count: 8,
        utc_offset_secs: IST,
        hours_24: false,
        auto_time: true,
        interactive: false,
        keyguard_locked: false,
        call: limo_phone::CallState::Idle,
        front: Front::Home,
        in_reach: true,
        prodded: false,
        permissions: PhonePermissions::ALL,
        guarded: None,
        usb_debugging: None,
    };
    assert!(engine.tick(&facts).send_report);
    let report = engine.outgoing_report(now).unwrap();
    assert!(engine.should_save());
    let ids: Vec<String> = report.notes.iter().map(|n| n.id.clone()).collect();
    assert_eq!(ids, (1..=11).map(|n| format!("phone/note/{n}")).collect::<Vec<_>>());
    assert!(matches!(report.notes[0].note, limo_core::PhoneNote::SetupFinished { .. }));
    assert!(matches!(report.notes[3].note, limo_core::PhoneNote::ClockMoved { by_secs: 3_600, followed: true, .. }));
    let on: Vec<limo_core::PhonePermission> = report.notes[4..10]
        .iter()
        .map(|n| match n.note {
            limo_core::PhoneNote::PermissionOn { permission, .. } => permission,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(on, limo_core::PhonePermission::ALL);
    assert!(matches!(report.notes[10].note, limo_core::PhoneNote::Protection { level: limo_core::ProtectionLevel::Base, .. }));
    let outcome = book.handle_report(&report, now).unwrap();
    assert_eq!(outcome.new_notes.len(), 11);
    assert_eq!(engine.take_grant(&outcome.grant, now).outcome, GrantOutcome::Taken);
    let view = engine.view().unwrap();
    assert!(matches!(view.notes.as_slice(), [PhoneNote::BlockedAppOpened { .. }]));
    // Kept and loaded again, it goes on numbering after them, and the phone
    // started again tells nothing of how it stands again.
    let mut again = PhoneEngine::load(&engine.save());
    again.tick(&PhoneFacts { wall_secs: now + 1, elapsed_ms: 20_000, boot_count: 9, ..facts.clone() });
    let report = again.outgoing_report(now + 1).unwrap();
    assert_eq!(report.notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["phone/note/12"]);
    assert!(matches!(report.notes[0].note, limo_core::PhoneNote::Restarted { .. }));
}
