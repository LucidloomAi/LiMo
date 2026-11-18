//! Whole days on one PC: a plain day to zero, idle pauses and full screen,
//! the always allowed list, midnight with the session in use, and a budget
//! changed in the middle of the day.

use limo_agent::{LockWhy, SessionState, Show};
use limo_core::{DailyBudget, WarningCause, WeekBudgets};
use limo_hub::AppUsage;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";

fn asha_at_the_pc(start: i64) -> (Pc, String) {
    let mut pc = Pc::new(start);
    let asha = pc.add_child("Asha", ASHA);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    (pc, asha)
}

fn locked_why(show: &Show) -> Option<LockWhy> {
    match show {
        Show::Locked { screen } => Some(screen.why),
        _ => None,
    }
}

fn seconds_left(show: &Show) -> Option<u64> {
    match show {
        Show::TimeLeft { seconds_left, .. } => *seconds_left,
        _ => None,
    }
}

#[test]
fn a_plain_day_runs_to_zero_with_each_warning_given_once() {
    let (mut pc, asha) = asha_at_the_pc(at(8, 0));
    pc.tick();
    assert_eq!(seconds_left(&pc.show(1)), Some(2 * HOUR as u64));
    while locked_why(&pc.show(1)).is_none() {
        assert!(pc.now() < at(11, 0), "never locked");
        pc.advance(1_000);
        pc.tick();
    }
    // Two hours of use, counted by the steady count from the first call.
    assert_eq!(pc.now(), at(10, 0));
    assert_eq!(locked_why(&pc.show(1)), Some(LockWhy::TimeUsedUp));
    let Show::Locked { screen } = pc.show(1) else { unreachable!() };
    assert!(screen.can_ask_for_time && screen.can_use_code && !screen.can_use_pin);
    assert_eq!(screen.message, "Today's screen time is used up.");
    // The next day, after any bedtime.
    assert_eq!(screen.until, Some(on(1, 0, 0)));

    // Each warning came once, at its point.
    let shown: Vec<(i64, WarningCause, u32, i64)> =
        pc.warnings.iter().map(|(_, when, w)| (*when, w.cause, w.minutes, w.lock_at)).collect();
    assert_eq!(shown, vec![
        (at(9, 45), WarningCause::TimeRunsOut, 15, at(10, 0)),
        (at(9, 55), WarningCause::TimeRunsOut, 5, at(10, 0)),
        (at(9, 59), WarningCause::TimeRunsOut, 1, at(10, 0)),
    ]);
    assert_eq!(pc.warnings[0].2.message, "15 minutes of screen time left today.");
    assert_eq!(pc.warnings[2].2.message, "1 minute of screen time left today.");

    // Asha stays at the lock screen: nothing more counts, and the lock stays.
    pc.run(10 * MIN);
    assert_eq!(locked_why(&pc.show(1)), Some(LockWhy::TimeUsedUp));
    assert_eq!(pc.counted(&asha), 2 * HOUR as u64);
    assert_eq!(pc.master_used(&asha), 2 * HOUR as u64);
    let today = day_of(pc.now());
    let usage = pc.hub().usage(&asha, today, today).unwrap();
    assert_eq!(usage.apps, vec![AppUsage { app_id: "minecraft.exe".into(), secs: 7_200, counted_secs: 7_200 }]);
    assert_eq!(pc.warnings.len(), 3);
    // A slice is about a minute of use, not a second.
    let slices: i64 = pc.raw(|conn| conn.query_row("SELECT count(*) FROM slice", [], |r| r.get(0)).unwrap());
    assert!((120..=140).contains(&slices), "{slices} slices");
    // The guardian sees the PC as this child's device on the master.
    let devices = pc.hub().devices(&asha).unwrap();
    assert_eq!((devices.len(), devices[0].name.as_str()), (1, "Family PC"));
}

#[test]
fn time_pauses_after_the_idle_minutes_unless_something_plays_in_full_screen() {
    let (mut pc, asha) = asha_at_the_pc(at(8, 0));
    pc.tick();
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 600);

    // Ten minutes with no input. The first three count; then the count
    // stops, exactly three minutes after the last input.
    pc.typing(1, false);
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 600 + 180);
    let Show::TimeLeft { counting, .. } = pc.show(1) else { panic!() };
    assert!(!counting);

    // Input again: counting starts at that call, and the step it came in
    // is not counted.
    pc.typing(1, true);
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), 600 + 180 + 300 - 1);

    // A film in full screen with no input counts all through.
    pc.session(1).facts.full_screen = true;
    pc.typing(1, false);
    pc.run(20 * MIN);
    assert_eq!(pc.counted(&asha), 600 + 180 + 299 + 1_200);

    // The guardian sets the idle minutes to 1: it holds from the next call.
    pc.session(1).facts.full_screen = false;
    pc.hub().set_idle_minutes(1).unwrap();
    let before = pc.counted(&asha);
    pc.run(2 * MIN);
    // The session had been idle twenty minutes already: nothing counts.
    assert_eq!(pc.counted(&asha), before);
    pc.typing(1, true);
    pc.run(30);
    pc.typing(1, false);
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), before + 29 + 60);

    // Windows locks the session: nothing counts while it is locked.
    pc.typing(1, true);
    pc.set_state(1, SessionState::Locked);
    let before = pc.counted(&asha);
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), before);
    // Everything reaches the master, at the latest when Asha logs off.
    pc.set_state(1, SessionState::InUse);
    pc.run(2 * MIN);
    let counted = pc.counted(&asha);
    pc.logout(1);
    pc.advance(1_000);
    pc.tick();
    assert_eq!(pc.master_used(&asha), counted);
}

#[test]
fn a_program_on_the_always_allowed_list_does_not_count() {
    let (mut pc, asha) = asha_at_the_pc(at(8, 0));
    pc.change_rules(&asha, |r| r.always_allowed_apps = vec!["calculatorapp.exe".into()]);
    pc.front(1, made("CalculatorApp.exe", "CalculatorApp.exe", "Microsoft Corporation"));
    pc.tick();
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), 0);
    let Show::TimeLeft { counting, seconds_left, .. } = pc.show(1) else { panic!() };
    assert!(!counting);
    assert_eq!(seconds_left, Some(2 * HOUR as u64));

    // A game renamed to the calculator's name is still the game.
    pc.front(1, made("CalculatorApp.exe", "Game.exe", "Some Studio"));
    pc.run(5 * MIN);
    // The step in which the program changed is the calculator's.
    assert_eq!(pc.counted(&asha), 5 * MIN as u64 - 1);
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), 599);

    pc.run(MIN);
    pc.logout(1);
    pc.advance(1_000);
    pc.tick();
    let today = day_of(pc.now());
    let usage = pc.hub().usage(&asha, today, today).unwrap();
    let by_app: Vec<(String, u64, u64)> = usage.apps.iter().map(|a| (a.app_id.clone(), a.secs, a.counted_secs)).collect();
    assert_eq!(by_app, vec![
        ("calculatorapp.exe".to_string(), 601, 0),
        ("minecraft.exe".to_string(), 359, 359),
        ("game.exe".to_string(), 300, 300),
    ]);
}

#[test]
fn midnight_with_the_session_in_use_starts_the_new_day_without_a_lock() {
    let (mut pc, asha) = asha_at_the_pc(at(23, 50));
    pc.tick();
    let mut shows = Vec::new();
    while pc.now() < on(1, 0, 10) {
        pc.advance(1_000);
        pc.tick();
        shows.push((pc.now(), pc.show(1)));
    }
    assert!(shows.iter().all(|(_, s)| matches!(s, Show::TimeLeft { .. })), "a lock at midnight");
    // Thursday's time left is its own budget, less Thursday's use.
    pc.run(MIN);
    assert_eq!(pc.counted(&asha), 660);
    assert_eq!(seconds_left(&pc.show(1)), Some(2 * HOUR as u64 - 660));
    // Each day counted its own part.
    pc.logout(1);
    pc.advance(1_000);
    pc.tick();
    let wednesday = day_of(at(12, 0));
    let thursday = wednesday + 1;
    assert_eq!(pc.hub().usage(&asha, wednesday, wednesday).unwrap().counted_secs, 600);
    assert_eq!(pc.hub().usage(&asha, thursday, thursday).unwrap().counted_secs, 660);
}

#[test]
fn a_budget_changed_in_the_middle_of_the_day_holds_from_the_next_call() {
    let (mut pc, asha) = asha_at_the_pc(at(8, 0));
    pc.tick();
    pc.run(70 * MIN);
    assert_eq!(pc.counted(&asha), 70 * MIN as u64);

    // Cut to an hour, below what was used: the PC finishes the lease it
    // holds and locks, as the core allows after a cut.
    pc.change_rules(&asha, |r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(60)));
    pc.tick();
    let mut locked_at = None;
    while locked_at.is_none() {
        pc.advance(1_000);
        pc.tick();
        if locked_why(&pc.show(1)).is_some() {
            locked_at = Some(pc.now());
        }
    }
    let at_lock = pc.counted(&asha);
    assert!(at_lock <= 80 * MIN as u64, "{at_lock}");
    assert_eq!(locked_why(&pc.show(1)), Some(LockWhy::TimeUsedUp));
    pc.run(5 * MIN);
    assert_eq!(pc.counted(&asha), at_lock);

    // Raised to three hours: open again at the next call.
    pc.change_rules(&asha, |r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(180)));
    pc.tick();
    assert_eq!(seconds_left(&pc.show(1)), Some(180 * MIN as u64 - at_lock));
    pc.run(10 * MIN);
    assert_eq!(pc.counted(&asha), at_lock + 600);

    // A day with no limit shows no time left, and still counts.
    pc.change_rules(&asha, |r| r.budgets = WeekBudgets::every_day(DailyBudget::NoLimit));
    pc.tick();
    let Show::TimeLeft { seconds_left, counting, .. } = pc.show(1) else { panic!() };
    assert_eq!((seconds_left, counting), (None, true));
    pc.run(MIN);
    assert_eq!(pc.counted(&asha), at_lock + 660);
}
