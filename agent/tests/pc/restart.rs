//! Restarts: the engine and the hub dropped and opened again at every kind
//! of moment. One script plays a whole day (use, idle, an always allowed
//! program, a blocked program, warnings, the lock, wrong codes and their
//! wait, a right code, a lock from the master and a lift, a request
//! answered, logging off and on, and midnight in use), uninterrupted, with a
//! restart after every single call, with restarts here and there, and with
//! clean stops. What is shown at every call, and what is counted, must be
//! the same every time.

use limo_agent::{Engine, LockWhy, SessionOrder, Show};
use limo_core::{DailyBudget, RequestReply, TryOutcome, WeekBudgets};
use limo_hub::Hub;

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Restarts {
    Never,
    /// A crash after every call.
    EveryCall,
    /// A crash after every call whose number this divides.
    Every(u64),
    /// A clean stop after every call whose number this divides.
    CleanEvery(u64),
}

struct Run {
    pc: Pc,
    asha: String,
    restarts: Restarts,
    /// What every call showed and counted.
    trace: Vec<String>,
    /// The engine was restarted since its last call.
    fresh: bool,
    n: u64,
}

/// What an order shows, without the ids each run makes for itself.
fn summary(order: &SessionOrder) -> String {
    let show = match &order.show {
        Show::Nothing => "nothing".to_string(),
        Show::TimeLeft { seconds_left, counting, warning, unlocked_until } => format!(
            "left {seconds_left:?} counting {counting} warning {:?} unlocked {unlocked_until:?}",
            warning.as_ref().map(|w| (w.cause, w.minutes, w.lock_at))
        ),
        Show::Locked { screen } => format!(
            "locked {:?} until {:?} ask {} code {} pin {} wait {:?} request {:?}",
            screen.why,
            screen.until,
            screen.can_ask_for_time,
            screen.can_use_code,
            screen.can_use_pin,
            screen.try_wait_until,
            screen.latest_request.as_ref().map(|r| (r.answer, r.request.minutes))
        ),
    };
    let close: Vec<String> = order.close.iter().map(|c| c.program.file_name.clone()).collect();
    format!("{} {show} close {close:?}", order.session_id)
}

impl Run {
    fn new(restarts: Restarts) -> Run {
        let mut pc = Pc::new(at(7, 0));
        let asha = pc.add_child("Asha", ASHA);
        pc.change_rules(&asha, |r| {
            r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(30));
            r.always_allowed_apps = vec!["calculatorapp.exe".into()];
            r.blocked_apps = vec!["robloxplayerbeta.exe".into()];
        });
        Run { pc, asha, restarts, trace: Vec::new(), fresh: false, n: 0 }
    }

    /// After every call of the engine, a tick or a way out.
    fn after_call(&mut self) {
        self.n += 1;
        match self.restarts {
            Restarts::Never => {}
            Restarts::EveryCall => self.crash(),
            Restarts::Every(k) if self.n.is_multiple_of(k) => self.crash(),
            Restarts::CleanEvery(k) if self.n.is_multiple_of(k) => {
                self.pc.stop_and_restart();
                self.fresh = true;
            }
            _ => {}
        }
    }

    fn crash(&mut self) {
        self.pc.restart();
        self.fresh = true;
    }

    /// One call, a second after the last.
    fn call(&mut self) {
        self.pc.advance(1_000);
        self.pc.tick();
        self.fresh = false;
        let counted = self.pc.engine().child_on_pc(&self.asha).map(|v| v.counted_secs);
        let shows: Vec<String> = self.pc.orders.sessions.iter().map(summary).collect();
        self.trace.push(format!("{} {shows:?} counted {counted:?}", self.pc.now()));
        self.after_call();
    }

    fn calls(&mut self, n: i64) {
        for _ in 0..n {
            self.call();
        }
    }

    fn until(&mut self, t: i64) {
        while self.pc.now() < t {
            self.call();
        }
    }

    fn until_locked(&mut self) {
        while !matches!(self.pc.show(1), Show::Locked { .. }) {
            assert!(self.trace.len() < 100_000);
            self.call();
        }
    }

    /// The PC is off, or nobody is on it: time passes with no calls.
    fn off_until(&mut self, t: i64) {
        let ms = u64::try_from(t - self.pc.now()).unwrap() * 1_000;
        self.pc.advance(ms);
    }

    /// A way out from the lock screen. After a restart the service has
    /// called the engine once, at the same moment, before any helper can act
    /// on its orders.
    fn way_out<T>(&mut self, act: impl FnOnce(&mut Engine, &mut Hub, i64) -> T) -> T {
        if self.fresh {
            self.pc.tick();
            self.fresh = false;
        }
        let now = self.pc.now();
        let (engine, hub) = self.pc.both();
        let out = act(engine, hub, now);
        self.after_call();
        out
    }

    fn device(&mut self) -> String {
        let asha = self.asha.clone();
        self.pc.hub().master_pc(&asha).unwrap().unwrap().device_id
    }

    /// The whole day.
    fn play(&mut self) {
        let pc = &mut self.pc;
        pc.login(1, ASHA, "asha");
        pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
        pc.tick();
        // Morning: ten minutes, five idle (three count), two in an always
        // allowed program, two more.
        self.calls(10 * MIN);
        self.pc.typing(1, false);
        self.calls(5 * MIN);
        self.pc.typing(1, true);
        self.pc.front(1, made("CalculatorApp.exe", "CalculatorApp.exe", "Microsoft Corporation"));
        self.calls(2 * MIN);
        self.pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
        self.calls(2 * MIN);
        self.pc.logout(1);
        self.calls(2);
        self.off_until(at(16, 0));

        // Afternoon: a blocked program renamed, closed; the day used up with
        // its warnings; wrong codes and their wait; a right code; a lock from
        // the master and a lift; a request answered.
        self.pc.login(1, ASHA, "asha");
        self.pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
        self.calls(5 * MIN);
        self.pc.session(1).facts.running.push(made("homework.exe", "RobloxPlayerBeta.exe", "Roblox Corporation"));
        self.calls(3);
        self.pc.session(1).facts.running.retain(|p| p.file_name != "homework.exe");
        self.until_locked();
        self.calls(30);
        for _ in 0..6 {
            self.way_out(|engine, hub, now| engine.try_code(hub, 1, "1111 2222", now).unwrap());
        }
        self.calls(61);
        let device = self.device();
        let now = self.pc.now();
        let code = self.pc.hub().make_code(&device, 15, now).unwrap();
        let outcome = self.way_out(|engine, hub, now| engine.try_code(hub, 1, &code.shown, now).unwrap());
        assert!(matches!(outcome, TryOutcome::Added { .. }), "{outcome:?}");
        self.calls(5 * MIN);
        let (asha, now) = (self.asha.clone(), self.pc.now());
        self.pc.hub().lock_now(&asha, None, now).unwrap();
        self.calls(MIN);
        let now = self.pc.now();
        self.pc.hub().lift(&asha, now).unwrap();
        self.until_locked();
        self.calls(10);
        let request = self.way_out(|engine, hub, now| engine.ask_for_time(hub, 1, 10, "homework", now).unwrap());
        self.calls(20);
        let now = self.pc.now();
        self.pc.hub().answer_request(&asha, &request.id, RequestReply::Grant { minutes: 10 }, now).unwrap();
        self.until_locked();
        self.calls(10);
        self.pc.logout(1);
        self.calls(2);
        self.off_until(at(23, 55));

        // Late: locked for the rest of Wednesday, then Thursday's time at
        // midnight, in use.
        self.pc.login(1, ASHA, "asha");
        self.pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
        self.until(on(1, 0, 5));
        self.pc.logout(1);
        self.calls(2);
    }

    /// Each day's counted use, as the master has it once all is reported.
    fn days(&mut self) -> (u64, u64) {
        let asha = self.asha.clone();
        let wednesday = day_of(at(12, 0));
        let hub = self.pc.hub();
        let wed = hub.usage(&asha, wednesday, wednesday).unwrap().counted_secs;
        let thu = hub.usage(&asha, wednesday + 1, wednesday + 1).unwrap().counted_secs;
        (wed, thu)
    }
}

fn play(restarts: Restarts) -> Run {
    let mut run = Run::new(restarts);
    run.play();
    run
}

#[test]
fn a_restart_after_every_single_call_for_a_whole_day_changes_nothing() {
    let mut calm = play(Restarts::Never);
    let mut restless = play(Restarts::EveryCall);
    assert!(restless.pc.restarts > 4_000, "{}", restless.pc.restarts);
    // The day as it should be: thirty minutes, fifteen from a code and ten
    // granted on Wednesday; five minutes on Thursday.
    assert_eq!(calm.days(), (55 * 60, 5 * 60));
    assert_eq!(restless.days(), calm.days());
    assert_eq!(calm.trace.len(), restless.trace.len());
    for (a, b) in calm.trace.iter().zip(&restless.trace) {
        assert_eq!(a, b);
    }
    // What a lock screen showed along the way.
    assert!(calm.trace.iter().any(|t| t.contains("locked GuardianLock")));
    assert!(calm.trace.iter().any(|t| t.contains("wait Some")));
    assert!(calm.trace.iter().any(|t| t.contains("homework.exe")));
    assert!(calm.trace.iter().any(|t| t.contains("warning Some((TimeRunsOut, 1,")));
}

#[test]
fn restarts_here_and_there_and_clean_stops_change_nothing() {
    let mut calm = play(Restarts::Never);
    for restarts in [Restarts::Every(7), Restarts::Every(97), Restarts::CleanEvery(13)] {
        let mut other = play(restarts);
        assert!(other.pc.restarts > 40, "{restarts:?}");
        assert_eq!(other.days(), calm.days(), "{restarts:?}");
        assert_eq!(other.trace, calm.trace, "{restarts:?}");
    }
}

#[test]
fn a_crash_loses_at_most_the_last_seconds_and_a_clean_stop_loses_nothing() {
    for clean in [false, true] {
        let mut pc = Pc::new(at(16, 0));
        let asha = pc.add_child("Asha", ASHA);
        pc.login(1, ASHA, "asha");
        pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
        pc.tick();
        pc.run(10 * MIN + 4);
        let counted = pc.counted(&asha);
        assert_eq!(counted, 604);
        // Stopped, and Asha logs off before the service is back.
        if clean {
            pc.stop_and_restart();
        } else {
            pc.restart();
        }
        pc.logout(1);
        pc.run(30);
        pc.login(1, ASHA, "asha");
        pc.run(1);
        let after = pc.counted(&asha);
        if clean {
            assert_eq!(after, counted);
        } else {
            assert!(counted - after <= 5, "{counted} then {after}");
        }
        // Either way the lock and the count go on from what was kept.
        assert!(matches!(pc.show(1), Show::TimeLeft { .. }));
        pc.run(MIN);
        assert_eq!(pc.counted(&asha), after + 60);
    }
}

#[test]
fn a_lock_that_was_on_is_on_again_at_the_first_call_after_a_crash() {
    let mut pc = Pc::new(at(16, 0));
    let asha = pc.add_child("Asha", ASHA);
    pc.change_rules(&asha, |r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(5)));
    pc.login(1, ASHA, "asha");
    pc.front(1, made("Minecraft.exe", "Minecraft.exe", "Mojang"));
    pc.tick();
    pc.run(5 * MIN);
    let Show::Locked { screen } = pc.show(1) else { panic!() };
    assert_eq!(screen.why, LockWhy::TimeUsedUp);
    for _ in 0..3 {
        pc.restart();
        pc.tick();
        let Show::Locked { screen: again } = pc.show(1) else { panic!("open after a restart") };
        assert_eq!(again, screen);
    }
    // And a guardian's lock, too.
    let now = pc.now();
    pc.hub().give_extra(&asha, 30, "tap", now).unwrap();
    pc.run(1);
    let now = pc.now();
    pc.hub().lock_now(&asha, None, now).unwrap();
    pc.run(1);
    pc.restart();
    pc.tick();
    let Show::Locked { screen } = pc.show(1) else { panic!("open after a restart") };
    assert_eq!(screen.why, LockWhy::GuardianLock);
    assert_eq!(pc.counted(&asha), 5 * 60 + 1);
}
