//! The seeded random run: many days on the fake PC that mix everything the
//! other stories do one at a time. Two children and a guardian switch
//! sessions; programs change, some always allowed, some blocked and
//! renamed; children go idle, watch in full screen and lock Windows; the
//! guardian gives extra time, locks, lifts, unlocks, changes the rules and
//! answers requests; codes and the PIN are typed, right and wrong; a phone
//! of the same child is used at home and away; the clock is set back and
//! forward, the time zone changes, the PC sleeps and the service is held
//! up; the service crashes, stops cleanly and the PC is turned off and on;
//! calls come one to nine seconds apart.
//!
//! The fake PC checks at every call that counted use never passes the
//! highest budget in play plus extra time (less use beyond what a device
//! held under an unlock), that no call gains more counted seconds than the
//! steady count allows, and that a lock screen is ordered exactly when the
//! core says the PC is locked. The run fails if it stops reaching the things
//! it is meant to mix.
//!
//! `LIMO_AGENT_SCENARIOS` (default 6) and `LIMO_AGENT_DAYS` (default 2) set
//! its size; `LIMO_AGENT_SEED` runs one scenario, and `LIMO_AGENT_TRACE`
//! prints what happens in it, call by call.

use std::collections::BTreeMap;

use limo_agent::{LockWhy, SessionState, Show};
use limo_core::{BlockedWindow, DailyBudget, PinAction, RequestReply, TryOutcome, WeekBudgets, Weekday};
use limo_hub::{AccountLink, UnlinkedAccounts};

use crate::common::*;

const ASHA: &str = "S-1-5-21-1001";
const RAVI: &str = "S-1-5-21-1002";
const MUM: &str = "S-1-5-21-1000";
const VISITOR: &str = "S-1-5-21-1009";

/// SplitMix64: small, seeded, and the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn range(&mut self, low: i64, high: i64) -> i64 {
        low + i64::try_from(self.below(u64::try_from(high - low + 1).unwrap())).unwrap()
    }

    /// True once in `n`.
    fn one_in(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }
}

/// What the run reached, to fail it when it stops reaching something.
#[derive(Default, Debug)]
struct Seen {
    calls: u64,
    counted_calls: u64,
    restarts: u64,
    clean_stops: u64,
    reboots: u64,
    locks: BTreeMap<String, u64>,
    warnings: u64,
    closes: u64,
    extra_given: u64,
    guardian_locks: u64,
    unlocks: u64,
    rule_changes: u64,
    requests: u64,
    granted: u64,
    codes_right: u64,
    codes_wrong: u64,
    pins: u64,
    phone_trips: u64,
    jumps: u64,
    zone_changes: u64,
    sleeps: u64,
    held_up: u64,
    switches: u64,
}

const PROGRAMS: [(&str, Option<&str>); 6] = [
    ("Minecraft.exe", Some("Minecraft.exe")),
    ("CalculatorApp.exe", Some("CalculatorApp.exe")),
    ("homework.exe", Some("RobloxPlayerBeta.exe")),
    ("game.exe", None),
    ("chrome.exe", Some("chrome.exe")),
    ("Code.exe", Some("Code.exe")),
];

struct World {
    pc: Pc,
    rng: Rng,
    asha: String,
    ravi: Option<String>,
    phone: Option<usize>,
    pin: bool,
    seen: Seen,
}

impl World {
    fn new(seed: u64) -> World {
        let mut rng = Rng(seed);
        let mut pc = Pc::new(on(0, 6, 0));
        let asha = pc.add_child("Asha", ASHA);
        pc.change_rules(&asha, |r| {
            r.always_allowed_apps = vec!["calculatorapp.exe".into()];
            r.blocked_apps = vec!["robloxplayerbeta.exe".into()];
        });
        let ravi = rng.one_in(2).then(|| pc.add_child("Ravi", RAVI));
        pc.hub().link_account(MUM, "Mum", AccountLink::NotLimited).unwrap();
        if rng.one_in(2) {
            pc.hub().set_unlinked_accounts(UnlinkedAccounts::Locked).unwrap();
        }
        let phone = (!rng.one_in(4)).then(|| pc.pair_phone(&asha));
        let pin = rng.one_in(2);
        if pin {
            let now = pc.now();
            pc.hub().set_pin(PIN, now).unwrap();
        }
        World { pc, rng, asha, ravi, phone, pin, seen: Seen::default() }
    }

    /// Notes what happened, when tracing.
    fn trace(&self, what: &str) {
        if std::env::var("LIMO_AGENT_TRACE").is_ok() {
            println!("{} at {}: {what}", self.pc.calls, self.pc.now());
        }
    }

    /// One call, `ms` after the last, and what it showed.
    fn call(&mut self, ms: u64) {
        self.pc.advance(ms);
        self.pc.tick();
        self.seen.calls += 1;
        for order in &self.pc.orders.sessions {
            match &order.show {
                Show::Locked { screen } => *self.seen.locks.entry(format!("{:?}", screen.why)).or_default() += 1,
                Show::TimeLeft { warning, counting, .. } => {
                    self.seen.warnings += u64::from(warning.is_some());
                    self.seen.counted_calls += u64::from(*counting);
                }
                Show::Nothing => {}
            }
            self.seen.closes += order.close.len() as u64;
        }
    }

    /// The session in use, if any.
    fn in_use(&self) -> Option<u32> {
        self.pc.sessions.iter().find(|s| s.facts.state == SessionState::InUse).map(|s| s.facts.session_id)
    }

    /// The lock screen of a child's session in use, if it is locked.
    fn locked_child_session(&self) -> Option<(u32, limo_agent::LockScreen)> {
        let id = self.in_use()?;
        let order = self.pc.orders.sessions.iter().find(|o| o.session_id == id)?;
        order.child_id.as_ref()?;
        match &order.show {
            Show::Locked { screen } if screen.why != LockWhy::AccountNotLinked => Some((id, screen.clone())),
            _ => None,
        }
    }

    fn child_of(&self, session_id: u32) -> Option<String> {
        self.pc.orders.sessions.iter().find(|o| o.session_id == session_id).and_then(|o| o.child_id.clone())
    }

    /// Someone sits down at the PC for a while.
    fn sit_down(&mut self) {
        let who = self.rng.below(12);
        let (session, account, name) = match who {
            0 => (3, MUM, "Mum"),
            1 => (4, VISITOR, "Visitor"),
            2..=4 if self.ravi.is_some() => (2, RAVI, "ravi"),
            _ => (1, ASHA, "asha"),
        };
        for s in &mut self.pc.sessions {
            if s.facts.state == SessionState::InUse {
                s.facts.state = SessionState::Disconnected;
            }
        }
        if self.pc.sessions.iter().any(|s| s.facts.session_id == session) {
            self.pc.set_state(session, SessionState::InUse);
            self.pc.typing(session, true);
        } else {
            self.pc.login(session, account, name);
        }
        let program = self.program();
        self.pc.front(session, program);
        self.trace(&format!("{name} sits down"));
        self.seen.switches += 1;
    }

    fn program(&mut self) -> limo_agent::Program {
        let (file, original) = PROGRAMS[self.rng.below(PROGRAMS.len() as u64) as usize];
        match original {
            Some(original) => made(file, original, "A maker"),
            None => program(file),
        }
    }

    /// Something happens, or nothing.
    fn something(&mut self) {
        let now = self.pc.now();
        let in_use = self.in_use();
        if self.rng.one_in(150) && let Some(s) = in_use {
            let p = self.program();
            self.pc.front(s, p);
        }
        if self.rng.one_in(250) && let Some(s) = in_use {
            let typing = !self.pc.session(s).typing;
            self.pc.typing(s, typing);
        }
        if self.rng.one_in(400) && let Some(s) = in_use {
            let full = !self.pc.session(s).facts.full_screen;
            self.pc.session(s).facts.full_screen = full;
        }
        if self.rng.one_in(500) && let Some(s) = in_use {
            self.pc.set_state(s, SessionState::Locked);
        } else if self.rng.one_in(60) {
            let locked: Vec<u32> = self
                .pc
                .sessions
                .iter()
                .filter(|s| s.facts.state == SessionState::Locked)
                .map(|s| s.facts.session_id)
                .collect();
            if let Some(&s) = locked.first() {
                self.pc.set_state(s, SessionState::InUse);
                self.pc.typing(s, true);
            }
        }
        if self.rng.one_in(600) {
            self.sit_down();
        }
        // A blocked program, renamed, starts in the background; the service
        // closes it a little later.
        if self.rng.one_in(500) && let Some(s) = in_use {
            self.pc.session(s).facts.running.push(made("homework.exe", "RobloxPlayerBeta.exe", "Roblox"));
        }
        if self.rng.one_in(20) {
            for s in &mut self.pc.sessions {
                s.facts.running.retain(|p| p.file_name != "homework.exe" || s.facts.in_front.as_ref() == Some(p));
            }
        }

        // The guardian.
        let child = if self.ravi.is_some() && self.rng.one_in(3) { self.ravi.clone().unwrap() } else { self.asha.clone() };
        if self.rng.one_in(1_200) {
            let minutes = u32::try_from(self.rng.range(5, 30)).unwrap();
            self.pc.hub().give_extra(&child, minutes, "", now).unwrap();
            self.trace(&format!("extra {minutes} for {child}"));
            self.seen.extra_given += 1;
        }
        if self.rng.one_in(2_000) {
            let until = if self.rng.one_in(2) { None } else { Some(now + self.rng.range(5, 30) * MIN) };
            self.pc.hub().lock_now(&child, until, now).unwrap();
            self.trace(&format!("lock {child} until {until:?}"));
            self.seen.guardian_locks += 1;
        }
        if self.rng.one_in(900) {
            self.pc.hub().lift(&child, now).unwrap();
            self.trace(&format!("lift {child}"));
        }
        if self.rng.one_in(2_500) {
            let until = now + self.rng.range(10, 40) * MIN;
            self.pc.hub().unlock_until(&child, until, now).unwrap();
            self.trace(&format!("unlock {child} until {until}"));
            self.seen.unlocks += 1;
        }
        if self.rng.one_in(900) {
            let budget = match self.rng.below(8) {
                0 => DailyBudget::NoLimit,
                1 => DailyBudget::Minutes(0),
                n => DailyBudget::Minutes(u32::try_from(n).unwrap() * 20),
            };
            let bedtime = self.rng.one_in(2);
            self.pc.change_rules(&child, |r| {
                r.budgets = WeekBudgets::every_day(budget);
                r.blocked_hours = if bedtime {
                    vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 21 * 60, end_minute: 7 * 60 }]
                } else {
                    Vec::new()
                };
            });
            self.trace(&format!("rules for {child}: {budget:?}, bedtime {bedtime}"));
            self.seen.rule_changes += 1;
        }
        if self.rng.one_in(200) {
            for waiting in self.pc.hub().waiting_requests(now).unwrap() {
                let reply = if self.rng.one_in(3) {
                    RequestReply::Refuse
                } else {
                    RequestReply::Grant { minutes: u32::try_from(self.rng.range(5, 20)).unwrap() }
                };
                if self.pc.hub().answer_request(&waiting.child_id, &waiting.request.request.id, reply, now).is_ok()
                    && reply != RequestReply::Refuse
                {
                    self.trace(&format!("granted {reply:?}"));
                    self.seen.granted += 1;
                }
            }
        }

        // The child at the lock screen.
        if let Some((session, screen)) = self.locked_child_session() {
            if screen.can_ask_for_time && self.rng.one_in(60) {
                let minutes = u32::try_from(self.rng.range(5, 30)).unwrap();
                let (engine, hub) = self.pc.both();
                if engine.ask_for_time(hub, session, minutes, "please", now).is_ok() {
                    self.seen.requests += 1;
                }
            }
            if screen.can_use_code && self.rng.one_in(300) {
                let child = self.child_of(session).unwrap();
                let device = self.pc.hub().master_pc(&child).unwrap().unwrap().device_id;
                if let Ok(code) = self.pc.hub().make_code(&device, 15, now) {
                    let (engine, hub) = self.pc.both();
                    if let TryOutcome::Added { .. } = engine.try_code(hub, session, &code.shown, now).unwrap() {
                        self.trace("code added 15 minutes");
                        self.seen.codes_right += 1;
                    }
                }
            }
            if screen.can_use_code && self.rng.one_in(150) {
                let (engine, hub) = self.pc.both();
                engine.try_code(hub, session, "9876 5432", now).unwrap();
                self.seen.codes_wrong += 1;
            }
            if self.pin && screen.can_use_pin && self.rng.one_in(150) {
                let action = if self.rng.one_in(2) {
                    PinAction::AddMinutes { minutes: 10 }
                } else {
                    PinAction::UnlockUntil { until: now + 15 * MIN }
                };
                let (engine, hub) = self.pc.both();
                if matches!(engine.try_pin(hub, session, PIN, action, now), Ok(TryOutcome::Added { .. } | TryOutcome::Unlocked { .. })) {
                    self.trace(&format!("PIN {action:?}"));
                    self.seen.pins += 1;
                }
            }
        }

        // The phone.
        if let Some(phone) = self.phone {
            if self.rng.one_in(800) {
                let home = !self.pc.phones[phone].home;
                self.pc.phone_home(phone, home);
                self.trace(&format!("phone home {home}"));
                self.seen.phone_trips += 1;
            }
            if self.rng.one_in(300) {
                let using = if self.pc.phones[phone].using.is_some() { None } else { Some("com.whatsapp".to_string()) };
                self.trace(&format!("phone using {using:?}"));
                self.pc.phones[phone].using = using;
            }
        }

        // The clock and the service.
        if self.rng.one_in(4_000) {
            self.pc.skew += if self.rng.one_in(2) { HOUR } else { -HOUR };
            self.trace(&format!("clock skew now {}", self.pc.skew));
            self.seen.jumps += 1;
        }
        if self.rng.one_in(6_000) {
            self.pc.offset = if self.pc.offset == IST { 0 } else { IST };
            self.seen.zone_changes += 1;
        }
        if self.rng.one_in(5_000) {
            let counts = self.rng.one_in(2);
            self.pc.sleep(self.rng.range(1, 3) * HOUR, counts);
            self.trace("sleep");
            self.seen.sleeps += 1;
        }
        if self.rng.one_in(800) {
            self.pc.advance(u64::try_from(self.rng.range(12, 40)).unwrap() * 1_000);
            self.seen.held_up += 1;
        }
        if self.rng.one_in(900) {
            self.pc.restart();
            self.trace("crash");
            self.seen.restarts += 1;
        } else if self.rng.one_in(1_500) {
            self.pc.stop_and_restart();
            self.trace("clean stop");
            self.seen.clean_stops += 1;
        }
    }

    /// The PC is turned off until `t`: the service stops, cleanly or not,
    /// everyone is logged off, and the steady count starts again at boot.
    fn off_until(&mut self, t: i64) {
        let ids: Vec<u32> = self.pc.sessions.iter().map(|s| s.facts.session_id).collect();
        for id in ids {
            self.pc.logout(id);
        }
        self.call(1_000);
        if self.rng.one_in(2) {
            self.pc.stop_and_restart();
        } else {
            self.pc.restart();
        }
        let ms = u64::try_from((t - self.pc.now()).max(0)).unwrap() * 1_000;
        self.pc.advance(ms);
        self.pc.steady_ms = 30_000 + self.rng.below(60_000);
        self.trace("reboot");
        self.seen.reboots += 1;
    }

    /// One day: the PC is on two or three times, with someone at it.
    fn day(&mut self, d: i64) {
        let mut periods = vec![
            (on(d, 7, 0) + self.rng.range(0, 90) * MIN, self.rng.range(10, 40) * MIN),
            (on(d, 15, 0) + self.rng.range(0, 120) * MIN, self.rng.range(60, 150) * MIN),
        ];
        if self.rng.one_in(2) {
            periods.push((on(d, 23, 30) + self.rng.range(0, 20) * MIN, self.rng.range(20, 50) * MIN));
        }
        for (start, length) in periods {
            if self.pc.now() < start {
                self.off_until(start);
            }
            self.sit_down();
            let end = self.pc.now() + length;
            while self.pc.now() < end {
                self.something();
                let ms = if self.rng.one_in(3) { 1_000 } else { u64::try_from(self.rng.range(1, 9)).unwrap() * 1_000 };
                self.call(ms);
            }
        }
    }
}

fn scenario(seed: u64, days: i64) -> Seen {
    let mut world = World::new(seed);
    for d in 0..days {
        world.day(d);
    }
    world.off_until(world.pc.now() + MIN);
    world.seen.calls = world.pc.calls;
    world.seen
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn a_seeded_random_run_over_many_days_keeps_every_promise() {
    let days = i64::try_from(env("LIMO_AGENT_DAYS", 2)).unwrap();
    let seeds: Vec<u64> = match std::env::var("LIMO_AGENT_SEED").ok().and_then(|v| v.parse().ok()) {
        Some(seed) => vec![seed],
        None => (0..env("LIMO_AGENT_SCENARIOS", 6)).map(|i| 0x11A0_0000 + i * 7_919).collect(),
    };
    let all: Vec<Seen> = std::thread::scope(|scope| {
        let runs: Vec<_> = seeds
            .iter()
            .map(|&seed| {
                scope.spawn(move || {
                    let result = std::panic::catch_unwind(|| scenario(seed, days));
                    result.unwrap_or_else(|_| panic!("scenario with seed {seed} failed; LIMO_AGENT_SEED={seed} runs it alone"))
                })
            })
            .collect();
        runs.into_iter().map(|run| run.join().unwrap()).collect()
    });
    let mut total = Seen::default();
    for seen in &all {
        total.calls += seen.calls;
        total.counted_calls += seen.counted_calls;
        total.restarts += seen.restarts;
        total.clean_stops += seen.clean_stops;
        total.reboots += seen.reboots;
        for (k, v) in &seen.locks {
            *total.locks.entry(k.clone()).or_default() += v;
        }
        total.warnings += seen.warnings;
        total.closes += seen.closes;
        total.extra_given += seen.extra_given;
        total.guardian_locks += seen.guardian_locks;
        total.unlocks += seen.unlocks;
        total.rule_changes += seen.rule_changes;
        total.requests += seen.requests;
        total.granted += seen.granted;
        total.codes_right += seen.codes_right;
        total.codes_wrong += seen.codes_wrong;
        total.pins += seen.pins;
        total.phone_trips += seen.phone_trips;
        total.jumps += seen.jumps;
        total.zone_changes += seen.zone_changes;
        total.sleeps += seen.sleeps;
        total.held_up += seen.held_up;
        total.switches += seen.switches;
    }
    println!("{} scenarios of {days} days: {total:#?}", all.len());
    // It reached what it is meant to mix.
    if std::env::var("LIMO_AGENT_SEED").is_err() {
        for (what, n) in [
            ("counted calls", total.counted_calls),
            ("restarts", total.restarts),
            ("reboots", total.reboots),
            ("warnings", total.warnings),
            ("closes", total.closes),
            ("extra", total.extra_given),
            ("rule changes", total.rule_changes),
            ("requests", total.requests),
            ("granted", total.granted),
            ("right codes", total.codes_right),
            ("PIN actions", total.pins),
            ("guardian locks", total.guardian_locks),
            ("unlocks", total.unlocks),
            ("wrong codes", total.codes_wrong),
            ("phone trips", total.phone_trips),
            ("held up", total.held_up),
            ("switches", total.switches),
        ] {
            assert!(n > 0, "the run never reached {what}: {total:#?}");
        }
        for why in ["TimeUsedUp", "AccountNotLinked"] {
            assert!(total.locks.get(why).is_some_and(|n| *n > 0), "never locked for {why}: {total:#?}");
        }
    }
}
