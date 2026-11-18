//! The ways out and a guardian's actions: extra time, a request answered, a
//! one time code, the PIN, lock now and lift, an unlock, and new rules, each
//! taking effect at the next call.

use limo_agent::{EngineError, LockScreen, LockWhy, Show};
use limo_core::{
    BlockedWindow, DailyBudget, DeviceError, PinAction, RequestAnswer, RequestReply, TryOutcome, WarningCause,
    WeekBudgets, Weekday,
};
use limo_hub::EventKind;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";

/// Asha at the PC with `minutes` a day, used up: the PC is locked.
fn used_up(minutes: u32) -> (Pc, String) {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.change_rules(&asha, |r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(minutes)));
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    pc.run(i64::from(minutes) * MIN);
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);
    (pc, asha)
}

fn lock(pc: &Pc) -> LockScreen {
    match pc.show(1) {
        Show::Locked { screen } => screen,
        other => panic!("not locked: {other:?}"),
    }
}

fn left(pc: &Pc) -> Option<u64> {
    match pc.show(1) {
        Show::TimeLeft { seconds_left, .. } => seconds_left,
        other => panic!("locked: {other:?}"),
    }
}

#[test]
fn extra_time_a_lock_and_a_lift_from_the_master_take_effect_at_the_next_call() {
    let (mut pc, asha) = used_up(30);
    let now = pc.now();
    pc.hub().give_extra(&asha, 10, "tap-1", now).unwrap();
    pc.run(1);
    assert_eq!(left(&pc), Some(600));
    // While counting, more extra time shows at the next call.
    pc.run(60);
    let now = pc.now();
    pc.hub().give_extra(&asha, 5, "tap-2", now).unwrap();
    pc.run(1);
    assert_eq!(left(&pc), Some(600 - 61 + 300));
    // The same tap again gives nothing.
    let now = pc.now();
    assert!(!pc.hub().give_extra(&asha, 5, "tap-2", now).unwrap().given_now);

    // Lock now: at the next call, with no ways out that ask for time.
    let now = pc.now();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.run(1);
    let screen = lock(&pc);
    assert_eq!(screen.why, LockWhy::GuardianLock);
    assert_eq!((screen.until, screen.can_ask_for_time), (None, false));
    let counted = pc.counted(&asha);
    pc.run(2 * MIN);
    assert_eq!(pc.counted(&asha), counted);
    // Lift: open again at the next call, and counting goes on.
    let now = pc.now();
    pc.hub().lift(&asha, now).unwrap();
    pc.run(1);
    assert!(matches!(pc.show(1), Show::TimeLeft { counting: true, .. }));
    pc.run(MIN);
    assert_eq!(pc.counted(&asha), counted + 60);
}

#[test]
fn an_unlock_from_the_master_keeps_the_pc_open_and_counts_beyond() {
    let (mut pc, asha) = used_up(20);
    let until = pc.now() + 30 * MIN;
    let now = pc.now();
    pc.hub().unlock_until(&asha, until, now).unwrap();
    pc.run(1);
    let Show::TimeLeft { unlocked_until, counting, .. } = pc.show(1) else { panic!() };
    assert_eq!((unlocked_until, counting), (Some(until), true));
    pc.run_until(until - 1);
    // The unlock's end is warned of, once for each point.
    let causes: Vec<(WarningCause, u32)> = pc.warnings.iter().map(|(_, _, w)| (w.cause, w.minutes)).collect();
    assert!(causes.ends_with(&[(WarningCause::UnlockEnds, 15), (WarningCause::UnlockEnds, 5), (WarningCause::UnlockEnds, 1)]));
    pc.run(2);
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);
    // Every second used was counted, beyond the day's twenty minutes: all
    // of the unlock but the step that ended at the first call after it was
    // set, which the lock still covered.
    let view = pc.view(&asha);
    assert_eq!(view.counted_secs, 20 * 60 + 30 * 60 - 1);
    assert_eq!(view.over_secs, 30 * 60 - 1);
    pc.run(2 * MIN);
    assert_eq!(pc.master_used(&asha), 50 * 60 - 1);
}

#[test]
fn a_request_is_answered_at_the_master_and_reaches_the_lock_screen() {
    let (mut pc, asha) = used_up(10);
    let screen = lock(&pc);
    assert!(screen.can_ask_for_time && screen.latest_request.is_none());

    let now = pc.now();
    let (engine, hub) = pc.both();
    let request = engine.ask_for_time(hub, 1, 20, "For the school project", now).unwrap();
    // It reached the master at once.
    let now = pc.now();
    let waiting = pc.hub().waiting_requests(now).unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!((waiting[0].request.request.id.as_str(), waiting[0].device_name.as_str()), (request.id.as_str(), "Family PC"));
    assert!(pc.events().iter().any(|e| matches!(e, EventKind::RequestArrived { minutes: 20, .. })));
    pc.run(1);
    let screen = lock(&pc);
    let latest = screen.latest_request.unwrap();
    assert_eq!((latest.answer, latest.reached_master, screen.can_ask_for_time), (RequestAnswer::Waiting, true, false));
    // Asking again while one waits is refused, in the core's words.
    let now = pc.now();
    let (engine, hub) = pc.both();
    let err = engine.ask_for_time(hub, 1, 10, "", now).unwrap_err();
    assert_eq!(err, EngineError::Refused { problem: DeviceError::RequestWaiting });
    assert_eq!(err.to_string(), "A request from this device is still waiting for an answer.");

    // Granted with fifteen minutes: open at the next call.
    let now = pc.now();
    pc.hub().answer_request(&asha, &request.id, RequestReply::Grant { minutes: 15 }, now).unwrap();
    pc.run(1);
    assert_eq!(left(&pc), Some(15 * 60));
    pc.run(15 * MIN);
    // Locked again; asked and refused: the lock screen says so, and may ask
    // again.
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);
    let now = pc.now();
    let (engine, hub) = pc.both();
    let second = engine.ask_for_time(hub, 1, 30, "", now).unwrap();
    let now = pc.now();
    pc.hub().answer_request(&asha, &second.id, RequestReply::Refuse, now).unwrap();
    pc.run(1);
    let screen = lock(&pc);
    assert_eq!(screen.latest_request.map(|r| r.answer), Some(RequestAnswer::Refused));
    assert!(screen.can_ask_for_time);

    // Only a child's session can ask.
    let now = pc.now();
    let (engine, hub) = pc.both();
    let err = engine.ask_for_time(hub, 9, 10, "", now).unwrap_err();
    assert_eq!(err, EngineError::NotAChildSession { session_id: 9 });
    assert_eq!(err.to_string(), "Session 9 is not a child's session that LiMo limits on this PC.");
}

#[test]
fn a_one_time_code_adds_time_once_and_wrong_codes_set_a_wait_that_survives_a_restart() {
    let (mut pc, asha) = used_up(10);
    let device = pc.view(&asha).device_id;
    let now = pc.now();
    let code = pc.hub().make_code(&device, 15, now).unwrap();
    let (engine, hub) = pc.both();
    let outcome = engine.try_code(hub, 1, &code.shown, now).unwrap();
    assert!(matches!(outcome, TryOutcome::Added { .. }), "{outcome:?}");
    pc.run(1);
    assert_eq!(left(&pc), Some(15 * 60));
    // The master has it as extra time from a code, once.
    let now = pc.now();
    let extra = pc.hub().extra_time_today(&asha, now).unwrap();
    assert_eq!(extra.iter().map(|e| e.secs).sum::<u64>(), 15 * 60);
    // Typed again: used, and not counted as a wrong guess.
    let (engine, hub) = pc.both();
    assert_eq!(engine.try_code(hub, 1, &code.shown, now).unwrap(), TryOutcome::AlreadyUsed);
    pc.run(15 * MIN);
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);

    // Five wrong codes: the fifth sets a wait of a minute.
    let now = pc.now();
    let (engine, hub) = pc.both();
    for n in 1..=5 {
        let outcome = engine.try_code(hub, 1, "1234 5678", now).unwrap();
        assert!(matches!(outcome, TryOutcome::Wrong { wrong_guesses, .. } if wrong_guesses == n), "{outcome:?}");
    }
    assert_eq!(engine.try_code(hub, 1, "1234 5678", now).unwrap(), TryOutcome::MustWait { until: now + 60 });
    pc.run(1);
    assert_eq!(lock(&pc).try_wait_until, Some(now + 60));
    // A crash and a restart change nothing: the wait and the count stay.
    pc.restart();
    pc.run(1);
    assert_eq!(lock(&pc).try_wait_until, Some(now + 60));
    let later = pc.now();
    let (engine, hub) = pc.both();
    assert_eq!(engine.try_code(hub, 1, "1234 5678", later).unwrap(), TryOutcome::MustWait { until: now + 60 });
    // The guardian sees the wrong guesses.
    let today = pc.hub().child_today(&asha, later).unwrap();
    assert_eq!(today.devices[0].wrong_guesses, 5);
    assert!(pc.events().iter().any(|e| matches!(e, EventKind::WrongGuesses { count: 5, .. })));
}

#[test]
fn the_pin_adds_time_unlocks_and_confirms_a_release() {
    let (mut pc, asha) = used_up(10);
    assert!(!lock(&pc).can_use_pin);
    let now = pc.now();
    pc.hub().set_pin(PIN, now).unwrap();
    pc.run(1);
    assert!(lock(&pc).can_use_pin);

    let now = pc.now();
    let (engine, hub) = pc.both();
    assert!(matches!(engine.try_pin(hub, 1, "000000", PinAction::AddMinutes { minutes: 10 }, now).unwrap(), TryOutcome::Wrong { .. }));
    let outcome = engine.try_pin(hub, 1, PIN, PinAction::AddMinutes { minutes: 10 }, now).unwrap();
    assert!(matches!(outcome, TryOutcome::Added { .. }));
    pc.run(1);
    assert_eq!(left(&pc), Some(600));
    pc.run(10 * MIN);
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);

    // An unlock until a time: open, counting, then locked again.
    let now = pc.now();
    let until = now + 20 * MIN;
    let (engine, hub) = pc.both();
    assert_eq!(engine.try_pin(hub, 1, PIN, PinAction::UnlockUntil { until }, now).unwrap(), TryOutcome::Unlocked { until });
    pc.run(1);
    assert!(matches!(pc.show(1), Show::TimeLeft { unlocked_until: Some(u), counting: true, .. } if u == until));
    pc.run_until(until + 1);
    assert_eq!(lock(&pc).why, LockWhy::TimeUsedUp);
    // An unlock that cannot be: refused before the PIN is looked at.
    let now = pc.now();
    let (engine, hub) = pc.both();
    let err = engine.try_pin(hub, 1, PIN, PinAction::UnlockUntil { until: now - 1 }, now).unwrap_err();
    assert_eq!(err, EngineError::Refused { problem: DeviceError::UnlockOutOfRange });
    // A release: the engine only says the PIN was right.
    assert_eq!(engine.try_pin(hub, 1, PIN, PinAction::ConfirmRelease, now).unwrap(), TryOutcome::ReleaseConfirmed);
    // The PIN removed at the master: the lock screen stops offering it.
    let now = pc.now();
    pc.hub().clear_pin(now).unwrap();
    pc.run(1);
    assert!(!lock(&pc).can_use_pin);
    let _ = asha;
}

#[test]
fn new_rules_take_effect_at_the_next_call_with_their_warnings() {
    let mut pc = Pc::new(at(20, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    pc.run(MIN);
    // Bedtime from 20:10 to 07:00, set at 20:01: ten minutes ahead, so the
    // fifteen minute warning comes at once.
    pc.change_rules(&asha, |r| {
        r.blocked_hours = vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 20 * 60 + 10, end_minute: 7 * 60 }]
    });
    pc.run(1);
    pc.run_until(at(20, 11));
    let shown: Vec<(i64, WarningCause, u32)> = pc.warnings.iter().map(|(_, when, w)| (*when, w.cause, w.minutes)).collect();
    assert_eq!(shown, vec![
        (at(20, 1) + 1, WarningCause::BlockedHoursStart, 15),
        (at(20, 5), WarningCause::BlockedHoursStart, 5),
        (at(20, 9), WarningCause::BlockedHoursStart, 1),
    ]);
    assert_eq!(pc.warnings[1].2.message, "The PC locks in 5 minutes, for the hours it is not used.");
    let screen = lock(&pc);
    assert_eq!((screen.why, screen.until, screen.can_ask_for_time), (LockWhy::BlockedHours, Some(on(1, 7, 0)), false));
    assert_eq!(pc.counted(&asha), 10 * 60);
}
