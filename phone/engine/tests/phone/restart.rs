//! Restarts: LiMo's process killed and started again at every kind of
//! moment, with only what the engine asked the host to keep. One script
//! plays a whole day (use at home, the home screen, an always allowed app,
//! a blocked app, leaving for school, the away time used up with its
//! warnings, wrong codes and their wait, a right code, a call while locked,
//! home again, the day used up, a request answered, a lock from the master
//! and a lift, and midnight in use), uninterrupted, with a crash after every
//! single call, and with crashes here and there. What every call orders,
//! and what is counted, must be the same every time.

use limo_core::{DailyBudget, RequestReply, TryOutcome, WeekBudgets};
use limo_phone::{CallState, Cover, Front, LockWhy, PhoneEngine, PhoneOrders};

use crate::common::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Restarts {
    Never,
    /// A crash after every call.
    EveryCall,
    /// A crash after every call whose number this divides.
    Every(u64),
}

struct Run {
    f: Family,
    restarts: Restarts,
    /// What every call ordered and counted.
    trace: Vec<String>,
    /// The engine was restarted since its last call.
    fresh: bool,
    n: u64,
}

/// What the orders say, without the ids each run makes for itself.
fn summary(o: &PhoneOrders) -> String {
    let cover = match &o.cover {
        Cover::Nothing => "nothing".to_string(),
        Cover::Block { package, call_on, .. } => format!("block {package} call {call_on}"),
        Cover::Lock(l) => format!(
            "lock {:?} call {} until {:?} ways {:?} wait {:?} request {:?}",
            l.why,
            l.call_on,
            l.until_words,
            l.ways.iter().map(|w| (w.way, w.off)).collect::<Vec<_>>(),
            l.wait_words,
            l.request_line.as_ref().map(|r| (r.text.clone(), r.stamp.clone()))
        ),
        Cover::Guard(g) => format!(
            "guard {:?} call {} ways {:?} wait {:?}",
            g.page,
            g.call_on,
            g.ways.iter().map(|w| (w.way, w.off)).collect::<Vec<_>>(),
            g.wait_words
        ),
    };
    format!(
        "{cover} | left {:?} counting {} {:?} | warning {:?}",
        o.time_left.seconds_left,
        o.time_left.counting,
        o.time_left.words,
        o.warning.as_ref().map(|w| (w.cause, w.minutes, w.lock_at, w.message.clone()))
    )
}

impl Run {
    fn new(restarts: Restarts) -> Run {
        let mut f = Family::new(at(7, 0));
        f.in_reach = true;
        f.set_rules(|r| {
            r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(60));
            r.always_allowed_apps = vec![CLOCK.into()];
            r.blocked_apps = vec![CASINO.into()];
        });
        Run { f, restarts, trace: Vec::new(), fresh: false, n: 0 }
    }

    /// After every call of the engine, a tick or a way out.
    fn after_call(&mut self) {
        self.n += 1;
        let crash = match self.restarts {
            Restarts::Never => false,
            Restarts::EveryCall => true,
            Restarts::Every(k) => self.n.is_multiple_of(k),
        };
        if crash {
            self.f.crash();
            self.fresh = true;
        }
    }

    /// One call, a second after the last.
    fn call(&mut self) {
        self.f.advance(1_000);
        self.f.tick();
        self.fresh = false;
        let line = format!("{} {} counted {}", self.f.now(), summary(&self.f.orders), self.f.counted());
        self.trace.push(line);
        self.after_call();
    }

    /// A fact changes: the host calls at once, at the same moment.
    fn change(&mut self, change: impl FnOnce(&mut Family)) {
        change(&mut self.f);
        self.f.tick();
        self.fresh = false;
        let line = format!("{} {} counted {}", self.f.now(), summary(&self.f.orders), self.f.counted());
        self.trace.push(line);
        self.after_call();
    }

    fn calls(&mut self, n: i64) {
        for _ in 0..n {
            self.call();
        }
    }

    fn until(&mut self, t: i64) {
        while self.f.now() < t {
            self.call();
        }
    }

    fn until_locked(&mut self) {
        while !matches!(self.f.orders.cover, Cover::Lock(_)) {
            assert!(self.trace.len() < 100_000);
            self.call();
        }
    }

    /// The phone lies with its screen off: time passes with no calls.
    fn off_until(&mut self, t: i64) {
        self.change(|f| f.screen_on = false);
        let ms = u64::try_from(t - self.f.now()).unwrap() * 1_000;
        self.f.advance(ms);
    }

    /// A way out from the lock. After a restart the host has called the
    /// engine once, at the same moment, before the lock can take a way out.
    fn way_out<T>(&mut self, act: impl FnOnce(&mut PhoneEngine, i64) -> T) -> T {
        if self.fresh {
            self.f.tick();
            self.fresh = false;
        }
        let out = self.f.way_out(act);
        self.after_call();
        out
    }

    /// The whole day.
    fn play(&mut self) {
        // Morning at home, in reach: a game, the home screen, the always
        // allowed clock, the blocked casino, the game again.
        self.change(|_| {});
        self.calls(5 * MIN);
        self.change(|f| f.front = Front::Home);
        self.calls(MIN);
        self.change(|f| f.front = Front::app(CLOCK));
        self.calls(2 * MIN);
        self.change(|f| f.front = Front::app(CASINO));
        self.calls(5);
        self.change(|f| f.front = Front::app(GAME));
        self.calls(2 * MIN);
        self.off_until(at(8, 0));

        // At school, out of reach: the away time used up with its warnings,
        // wrong codes and their wait, a right code, a call while locked.
        self.change(|f| {
            f.in_reach = false;
            f.screen_on = true;
        });
        self.until_locked();
        self.calls(30);
        for _ in 0..6 {
            self.way_out(|e, now| e.try_code("1111 2222", now).unwrap());
        }
        self.calls(61);
        let (phone, now) = (self.f.phone_id.clone(), self.f.now());
        let code = self.f.hub().make_code(&phone, 15, now).unwrap();
        let answer = self.way_out(|e, now| e.try_code(&code.shown, now).unwrap());
        assert!(matches!(answer.outcome, TryOutcome::Added { .. }), "{answer:?}");
        // The code's quarter of an hour, then a call while locked: ringing,
        // then in progress with the game in front, covered all the same.
        self.call();
        self.until_locked();
        self.change(|f| f.call = CallState::Ringing);
        self.calls(10);
        self.change(|f| f.call = CallState::Ongoing);
        self.calls(20);
        self.change(|f| f.call = CallState::Idle);
        self.calls(5);
        self.off_until(at(16, 0));

        // Home: the day used up, a request answered, a lock from the master
        // and a lift.
        self.change(|f| {
            f.in_reach = true;
            f.screen_on = true;
        });
        self.until_locked();
        self.calls(10);
        let request = self.way_out(|e, now| e.ask_for_time(10, "homework", now).unwrap());
        self.calls(20);
        let (child, now) = (self.f.child.clone(), self.f.now());
        self.f.hub().answer_request(&child, &request.id, RequestReply::Grant { minutes: 10 }, now).unwrap();
        self.calls(5);
        self.until_locked();
        self.calls(10);
        let now = self.f.now();
        self.f.hub().give_extra(&child, 30, "tap", now).unwrap();
        self.calls(MIN);
        let now = self.f.now();
        self.f.hub().lock_now(&child, None, now).unwrap();
        self.calls(MIN);
        let now = self.f.now();
        self.f.hub().lift(&child, now).unwrap();
        // The phone hears of the lift at its next call, a second on.
        self.calls(MIN + 1);
        self.off_until(at(23, 55));

        // Late: locked for the rest of Wednesday, then Thursday's time at
        // midnight, in use.
        self.change(|f| f.screen_on = true);
        self.until(on(1, 0, 5));
        self.change(|f| f.screen_on = false);
        // A minute more at home, so that the master has every second.
        self.calls(MIN + 1);
    }

    /// Each day's counted use, as the master has it once all is reported.
    fn days(&mut self) -> (u64, u64) {
        let child = self.f.child.clone();
        let wednesday = day_of(at(12, 0));
        let hub = self.f.hub();
        let wed = hub.usage(&child, wednesday, wednesday).unwrap().counted_secs;
        let thu = hub.usage(&child, wednesday + 1, wednesday + 1).unwrap().counted_secs;
        (wed, thu)
    }
}

fn play(restarts: Restarts) -> Run {
    let mut run = Run::new(restarts);
    run.play();
    run
}

#[test]
fn a_crash_after_every_single_call_for_a_whole_day_changes_nothing() {
    let mut calm = play(Restarts::Never);
    let mut restless = play(Restarts::EveryCall);
    assert!(restless.f.restarts > 4_000, "{}", restless.f.restarts);
    // The day as it should be: the hour of the budget, fifteen minutes from
    // a code and ten granted; then two of the thirty given at home (the
    // minute before the lock from the master and the minute after the lift)
    // and the five before midnight; five minutes on Thursday.
    assert_eq!(calm.days(), ((60 + 15 + 10 + 2 + 5) * 60, 5 * 60));
    assert_eq!(restless.days(), calm.days());
    assert_eq!(calm.trace.len(), restless.trace.len());
    for (a, b) in calm.trace.iter().zip(&restless.trace) {
        assert_eq!(a, b);
    }
    // What the phone showed along the way.
    for seen in [
        "lock AwayTimeUsedUp",
        "lock AwayTimeUsedUp call true",
        "lock TimeUsedUp",
        "lock GuardianLock",
        "block org.example.casino",
        "wait Some(",
        "\"Given 10 min\"",
        "(TimeRunsOut, 1,",
        "\"1 minute of away time left today.\"",
    ] {
        assert!(calm.trace.iter().any(|t| t.contains(seen)), "never showed {seen}");
    }
}

#[test]
fn crashes_here_and_there_change_nothing() {
    let mut calm = play(Restarts::Never);
    for restarts in [Restarts::Every(7), Restarts::Every(97)] {
        let mut other = play(restarts);
        assert!(other.f.restarts > 40, "{restarts:?}");
        assert_eq!(other.days(), calm.days(), "{restarts:?}");
        assert_eq!(other.trace, calm.trace, "{restarts:?}");
    }
}

#[test]
fn a_crash_loses_at_most_the_last_five_seconds_and_a_lock_is_on_again_at_once() {
    let mut f = Family::new(at(16, 0));
    f.set_rules(|r| r.budgets = WeekBudgets::every_day(DailyBudget::Minutes(10)));
    f.run(3 * MIN + 2);
    let counted = f.counted();
    assert_eq!(counted, 3 * 60 + 2);
    // Killed, and the screen goes off before LiMo is back.
    f.crash();
    f.screen_on = false;
    f.advance(30_000);
    f.tick();
    let after = f.counted();
    assert!(counted - after <= 5, "{counted} then {after}");
    f.screen_on = true;
    f.run_until_locked();
    let lock = f.lock();
    assert_eq!(lock.why, LockWhy::AwayTimeUsedUp);
    for _ in 0..3 {
        f.crash();
        f.tick();
        assert_eq!(f.lock(), lock, "open after a restart");
    }
    // The state file holds no more than the engine asked to keep.
    let again = PhoneEngine::load(&f.kept);
    assert_eq!(again.load_problem(), None);
}
