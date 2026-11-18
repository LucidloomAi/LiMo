//! The fake PC: a family file, an engine, a wall clock and a steady count
//! of its own, Windows sessions and phones, driven call by call. Every call
//! checks what must always hold:
//!
//! - the orders came with no problem (unless a story allows them);
//! - every child's session is locked exactly when the core says the PC is
//!   locked for that child;
//! - a child's counted use on the PC never goes down within a day, and never
//!   grows by more than the steady count allows;
//! - when asked, the time that counts on all of a child's devices, reported
//!   or not, is within the highest budget in play plus the extra time.

use std::collections::BTreeMap;
use std::path::PathBuf;

use limo_agent::{ChildOnPc, Engine, Facts, Orders, Program, Session, SessionOrder, SessionState, Show, WarningShown};
use limo_core::{CivilDate, DeviceKind, DeviceState, GrantOutcome, RuleSet};
use limo_hub::{AccountLink, AgeBand, EventKind, Hub};
use tempfile::TempDir;

/// A minute, in seconds.
pub const MIN: i64 = 60;
/// An hour, in seconds.
pub const HOUR: i64 = 3_600;
/// India, the home zone of every story.
pub const IST: i32 = 19_800;
/// The guardian's password in every story.
pub const PASSWORD: &str = "correct horse battery";
/// The guardian's PIN in every story.
pub const PIN: &str = "482915";

/// `days` after Wednesday 18 November 2026, at `hour:minute`, home time.
pub fn on(days: i64, hour: u8, minute: u8) -> i64 {
    RuleSet::new(1, IST).calendar().at(CivilDate::new(2026, 11, 18).unwrap(), hour, minute, 0) + days * 86_400
}

/// Wednesday 18 November 2026 at `hour:minute`, home time.
pub fn at(hour: u8, minute: u8) -> i64 {
    on(0, hour, minute)
}

/// The home day of a moment.
pub fn day_of(t: i64) -> i64 {
    RuleSet::new(1, IST).calendar().day_of(t)
}

/// A program with no name recorded inside it.
pub fn program(file: &str) -> Program {
    Program { file_name: file.into(), original_name: None, maker: None }
}

/// A program whose maker recorded its original name and their own.
pub fn made(file: &str, original: &str, maker: &str) -> Program {
    Program { file_name: file.into(), original_name: Some(original.into()), maker: Some(maker.into()) }
}

/// A session on the fake PC: the facts, and whether someone is at the
/// keyboard.
pub struct FakeSession {
    pub facts: Session,
    /// True while someone types or moves the mouse.
    pub typing: bool,
    idle_ms: u64,
}

/// A phone of a child, driven against the same family file. It uses time
/// only while its own decision allows it, and only what it holds.
pub struct Phone {
    pub id: String,
    pub child_id: String,
    pub state: DeviceState,
    /// In reach of the master PC.
    pub home: bool,
    /// The app in use, if any.
    pub using: Option<String>,
    last_report: i64,
    last_ask: i64,
    last_use: i64,
    /// Use beyond what the phone held, under a guardian's unlock, by home
    /// day, counted as it happens. The phone's holding cannot tell it later:
    /// the master never sets an away allowance below what was used, so it
    /// may raise the allowance to cover that use.
    pub beyond_by_day: BTreeMap<i64, u64>,
}

impl Phone {
    /// One exchange: the phone reports, the hub answers, the phone takes the
    /// grant, and acknowledges at once when the grant asks.
    pub fn exchange(&mut self, hub: &mut Hub, now: i64, want_more: bool) {
        let report = self.state.report(now, want_more);
        let grant = hub.handle_report(&report, now).unwrap();
        self.state.take_grant(&grant, now);
        if want_more {
            self.last_ask = now;
        }
        if grant.acknowledge_now {
            let report = self.state.report(now, want_more);
            let again = hub.handle_report(&report, now).unwrap();
            assert_ne!(self.state.take_grant(&again, now), GrantOutcome::NotForThisDevice);
        }
        self.last_report = now;
    }

    /// Counted use on this phone today, reported or not.
    pub fn used_today(&self, now: i64) -> u64 {
        self.state.holding_at(now).used_secs()
    }
}

/// The fake PC.
pub struct Pc {
    _dir: TempDir,
    pub path: PathBuf,
    hub: Option<Hub>,
    engine: Option<Engine>,
    /// The true time, unix milliseconds.
    true_ms: i64,
    /// How far the wall clock is from the true time, in seconds.
    pub skew: i64,
    pub steady_ms: u64,
    /// The PC's time zone, as the facts give it.
    pub offset: i32,
    pub sessions: Vec<FakeSession>,
    pub phones: Vec<Phone>,
    /// The orders of the last call.
    pub orders: Orders,
    /// Every warning shown, with its session and when, in order.
    pub warnings: Vec<(u32, i64, WarningShown)>,
    /// Restart the engine and the hub after every call.
    pub restart_every_call: bool,
    /// Let the orders carry problems.
    pub allow_problems: bool,
    /// Check the budget of every child at every call.
    pub check_budget: bool,
    /// Calls made, and restarts.
    pub calls: u64,
    pub restarts: u64,
    /// Every rule set given to each child, to know the highest budget in
    /// play.
    rules_given: BTreeMap<String, Vec<RuleSet>>,
    /// Each child's device's day and counted use on the PC at the last
    /// call, and the steady count then.
    counted: BTreeMap<String, (i64, u64)>,
    /// Each child's device's use on the PC beyond what it held, under a
    /// guardian's unlock, by day, as last seen: it stays known after the
    /// child leaves the PC and the engine lets go of the device.
    pc_beyond: BTreeMap<(String, String, i64), u64>,
    last_steady: u64,
}

impl Pc {
    /// A fake PC with a family set up, at `start`.
    pub fn new(start: i64) -> Pc {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("family.limo");
        let mut hub = Hub::open(&path).unwrap();
        hub.create_family(PASSWORD, start).unwrap();
        Pc {
            _dir: dir,
            path,
            hub: Some(hub),
            engine: Some(Engine::new("Family PC")),
            true_ms: start * 1_000,
            skew: 0,
            steady_ms: 7_200_000,
            offset: IST,
            sessions: Vec::new(),
            phones: Vec::new(),
            orders: Orders::default(),
            warnings: Vec::new(),
            restart_every_call: false,
            allow_problems: false,
            check_budget: true,
            calls: 0,
            restarts: 0,
            rules_given: BTreeMap::new(),
            counted: BTreeMap::new(),
            pc_beyond: BTreeMap::new(),
            last_steady: 0,
        }
    }

    pub fn hub(&mut self) -> &mut Hub {
        self.hub.as_mut().unwrap()
    }

    pub fn engine(&mut self) -> &mut Engine {
        self.engine.as_mut().unwrap()
    }

    /// The engine and the hub together, for a way out.
    pub fn both(&mut self) -> (&mut Engine, &mut Hub) {
        (self.engine.as_mut().unwrap(), self.hub.as_mut().unwrap())
    }

    /// What the wall clock reads now.
    pub fn now(&self) -> i64 {
        self.true_ms.div_euclid(1_000) + self.skew
    }

    /// A child, with a Windows account linked to it.
    pub fn add_child(&mut self, name: &str, account: &str) -> String {
        let now = self.now();
        let child = self.hub().add_child(name, AgeBand::TenToTwelve, IST, now).unwrap().child_id;
        self.hub().link_account(account, name, AccountLink::Child { child_id: child.clone() }).unwrap();
        let rules = self.hub().rules(&child).unwrap();
        self.rules_given.insert(child.clone(), vec![rules]);
        child
    }

    /// Changes a child's rules at the master.
    pub fn change_rules(&mut self, child: &str, change: impl FnOnce(&mut RuleSet)) -> RuleSet {
        let now = self.now();
        let mut rules = self.hub().rules(child).unwrap();
        change(&mut rules);
        let taken = self.hub().replace_rules(child, rules, now).unwrap();
        self.rules_given.entry(child.to_string()).or_default().push(taken.clone());
        taken
    }

    /// Someone logs in to `account` and is at the keyboard.
    pub fn login(&mut self, session_id: u32, account: &str, name: &str) {
        self.sessions.push(FakeSession {
            facts: Session {
                session_id,
                account_id: account.into(),
                account_name: name.into(),
                state: SessionState::InUse,
                idle_secs: 0,
                full_screen: false,
                in_front: None,
                running: Vec::new(),
            },
            typing: true,
            idle_ms: 0,
        });
    }

    pub fn logout(&mut self, session_id: u32) {
        self.sessions.retain(|s| s.facts.session_id != session_id);
    }

    pub fn session(&mut self, session_id: u32) -> &mut FakeSession {
        self.sessions.iter_mut().find(|s| s.facts.session_id == session_id).unwrap()
    }

    /// `program` comes to the front of a session, and runs there.
    pub fn front(&mut self, session_id: u32, program: Program) {
        let session = &mut self.session(session_id).facts;
        if !session.running.contains(&program) {
            session.running.push(program.clone());
        }
        session.in_front = Some(program);
    }

    pub fn set_state(&mut self, session_id: u32, state: SessionState) {
        self.session(session_id).facts.state = state;
    }

    pub fn typing(&mut self, session_id: u32, typing: bool) {
        let session = self.session(session_id);
        session.typing = typing;
        if typing {
            session.idle_ms = 0;
            session.facts.idle_secs = 0;
        }
    }

    /// Time passes, by `ms`, on the true clock and the steady count.
    pub fn advance(&mut self, ms: u64) {
        self.true_ms += ms as i64;
        self.steady_ms += ms;
        for s in &mut self.sessions {
            s.idle_ms = if s.typing { 0 } else { s.idle_ms + ms };
            s.facts.idle_secs = u32::try_from(s.idle_ms / 1_000).unwrap();
        }
    }

    /// The PC sleeps for `secs`: the true clock moves; the steady count
    /// moves too when it counts through sleep. The session wakes up locked
    /// by Windows.
    pub fn sleep(&mut self, secs: i64, steady_counts: bool) {
        self.true_ms += secs * 1_000;
        if steady_counts {
            self.steady_ms += secs as u64 * 1_000;
        }
        for s in &mut self.sessions {
            s.facts.state = SessionState::Locked;
            s.idle_ms += secs as u64 * 1_000;
            s.facts.idle_secs = u32::try_from(s.idle_ms / 1_000).unwrap_or(u32::MAX);
        }
    }

    /// Restarts the engine and the hub as a crash would: both are dropped,
    /// with nothing saved on the way out, and opened again on the same file.
    /// A crash may lose the last few seconds counted, so the check that a
    /// count never goes down starts again after it.
    pub fn restart(&mut self) {
        self.reopen();
        self.counted.clear();
    }

    fn reopen(&mut self) {
        drop(self.engine.take());
        drop(self.hub.take());
        self.hub = Some(Hub::open(&self.path).unwrap());
        self.engine = Some(Engine::new("Family PC"));
        self.restarts += 1;
    }

    /// A clean stop and start: the engine keeps everything first, so
    /// nothing counted is lost.
    pub fn stop_and_restart(&mut self) {
        let (engine, hub) = self.both();
        assert!(engine.stop(hub).is_empty());
        self.reopen();
    }

    /// The facts as they stand.
    pub fn facts(&self) -> Facts {
        Facts {
            wall_secs: self.now(),
            steady_ms: self.steady_ms,
            utc_offset_secs: self.offset,
            sessions: self.sessions.iter().map(|s| s.facts.clone()).collect(),
        }
    }

    /// One call, and the checks.
    pub fn tick(&mut self) -> &Orders {
        let facts = self.facts();
        let (engine, hub) = self.both();
        let orders = engine.tick(hub, &facts);
        self.calls += 1;
        if !self.allow_problems {
            assert!(orders.problems.is_empty(), "problems at call {}: {:?}", self.calls, orders.problems);
        }
        assert_eq!(orders.sessions.len(), facts.sessions.len());
        for order in &orders.sessions {
            if let Show::TimeLeft { warning: Some(w), .. } = &order.show {
                self.warnings.push((order.session_id, facts.wall_secs, w.clone()));
            }
        }
        self.orders = orders;
        self.check_locks();
        self.check_counting();
        self.drive_phones();
        if self.check_budget {
            let children: Vec<String> = self.rules_given.keys().cloned().collect();
            for child in children {
                self.check_budget_of(&child);
            }
        }
        if self.restart_every_call {
            self.restart();
        }
        &self.orders
    }

    /// `secs` calls, a second apart.
    pub fn run(&mut self, secs: i64) {
        for _ in 0..secs {
            self.advance(1_000);
            self.tick();
        }
    }

    /// Calls a second apart until the wall clock reads `t`.
    pub fn run_until(&mut self, t: i64) {
        while self.now() < t {
            self.advance(1_000);
            self.tick();
        }
    }

    /// The order of the last call for a session.
    pub fn order(&self, session_id: u32) -> SessionOrder {
        self.orders.sessions.iter().find(|o| o.session_id == session_id).cloned().unwrap()
    }

    pub fn show(&self, session_id: u32) -> Show {
        self.order(session_id).show
    }

    /// The engine's view of a child.
    pub fn view(&self, child: &str) -> ChildOnPc {
        self.engine.as_ref().unwrap().child_on_pc(child).unwrap()
    }

    /// Counted use on the PC today, reported or not.
    pub fn counted(&self, child: &str) -> u64 {
        self.view(child).counted_secs
    }

    /// Counted use today on all of a child's devices, as the master has it.
    pub fn master_used(&mut self, child: &str) -> u64 {
        let now = self.now();
        self.hub().child_today(child, now).unwrap().used_secs
    }

    /// Looks inside the family's file with the hub closed; the engine keeps
    /// what it holds, as it would while the service's hub is reopened.
    pub fn raw<T>(&mut self, look: impl FnOnce(&rusqlite::Connection) -> T) -> T {
        drop(self.hub.take());
        let conn = rusqlite::Connection::open(&self.path).unwrap();
        let out = look(&conn);
        drop(conn);
        self.hub = Some(Hub::open(&self.path).unwrap());
        out
    }

    /// The events, newest first.
    pub fn events(&mut self) -> Vec<EventKind> {
        self.hub().events(500, None).unwrap().into_iter().map(|e| e.what).collect()
    }

    /// Pairs a phone with a child, at home.
    pub fn pair_phone(&mut self, child: &str) -> usize {
        let now = self.now();
        let pairing = self.hub().pair_device(child, "Phone", DeviceKind::Portable, "android", now).unwrap();
        let mut state = DeviceState::from_pairing(&pairing.welcome, now);
        state.set_code_secret(pairing.code_secret).unwrap();
        self.phones.push(Phone {
            id: pairing.device_id,
            child_id: child.to_string(),
            state,
            home: true,
            using: None,
            last_report: now,
            last_ask: i64::MIN / 2,
            last_use: now,
            beyond_by_day: BTreeMap::new(),
        });
        self.phones.len() - 1
    }

    /// The phone leaves home, or comes back and reports at once.
    pub fn phone_home(&mut self, phone: usize, home: bool) {
        let now = self.now();
        let mut phones = std::mem::take(&mut self.phones);
        let p = &mut phones[phone];
        p.home = home;
        if home {
            let want = p.using.is_some();
            p.exchange(self.hub(), now, want);
        } else {
            p.state.lost_contact();
        }
        self.phones = phones;
    }

    /// Every lock the core gives is a lock screen, and only then.
    fn check_locks(&self) {
        let engine = self.engine.as_ref().unwrap();
        for order in &self.orders.sessions {
            let Some(child) = &order.child_id else { continue };
            let view = engine.child_on_pc(child);
            match view.as_ref().and_then(|v| v.decision.clone()) {
                None => assert!(
                    matches!(&order.show, Show::Locked { screen } if screen.why == limo_agent::LockWhy::CannotCheck),
                    "{order:?}"
                ),
                Some(decision) => match decision.device_lock {
                    Some(_) => assert!(matches!(order.show, Show::Locked { .. }), "core says locked: {order:?}"),
                    None => assert!(matches!(order.show, Show::TimeLeft { .. }), "core says open: {order:?}"),
                },
            }
            assert!(order.helper_needed);
        }
    }

    /// Counted use never goes down within a day, and never grows by more
    /// than the steady count allows.
    fn check_counting(&mut self) {
        let engine = self.engine.as_ref().unwrap();
        let step = self.steady_ms.saturating_sub(self.last_steady);
        let allowed = step.div_ceil(1_000);
        for child in engine.children_on_pc() {
            let view = engine.child_on_pc(&child).unwrap();
            let key = format!("{child}/{}", view.device_id);
            if let Some((day, before)) = self.counted.get(&key)
                && *day == view.day
            {
                assert!(view.counted_secs >= *before, "counted went down at call {}: {before} to {}", self.calls, view.counted_secs);
                assert!(
                    view.counted_secs - before <= allowed,
                    "call {} counted {} seconds in a step of {step} ms",
                    self.calls,
                    view.counted_secs - before
                );
            }
            self.pc_beyond.insert((child.clone(), view.device_id.clone(), view.day), view.over_secs);
            self.counted.insert(key, (view.day, view.counted_secs));
        }
        self.last_steady = self.steady_ms;
    }

    /// The phones at home report when the hub wants them to, and every
    /// minute, and ask for time when they run low while in use (again each
    /// minute while still low). A phone in use spends what it holds.
    fn drive_phones(&mut self) {
        let now = self.now();
        let prod = self.orders.prod_devices.clone();
        let mut phones = std::mem::take(&mut self.phones);
        for p in &mut phones {
            if let Some(app) = p.using.clone() {
                let from = p.last_use.max(now - 10);
                if now > from {
                    let d = p.state.decide(from, Some(&app));
                    if d.allowed {
                        let can = match (d.seconds_in_hand, d.unlocked_until) {
                            (Some(h), None) => (now - from).min(h as i64),
                            _ => now - from,
                        };
                        if can > 0 {
                            p.state.record_use(&app, from, from + can);
                            if let (Some(held), Some(_)) = (d.seconds_in_hand, d.unlocked_until)
                                && d.counts
                            {
                                let beyond = (can as u64).saturating_sub(held);
                                *p.beyond_by_day.entry(d.day).or_default() += beyond;
                            }
                        }
                    }
                }
            }
            p.last_use = now;
            if p.home {
                let d = p.state.decide(now, p.using.as_deref());
                // Asks when it runs low while in use, and again each minute
                // while it is still low, as the core's retry says.
                let ask = d.ask_master && p.using.is_some() && now - p.last_ask >= limo_core::RETRY_SECS;
                if prod.contains(&p.id) || now - p.last_report >= 60 || ask {
                    p.exchange(self.hub(), now, ask);
                }
            }
        }
        self.phones = phones;
    }

    /// The highest budget a child's rules gave for `day`, or None for no
    /// limit in some rules.
    fn highest_budget(&self, child: &str, day: i64) -> Option<u64> {
        let given = self.rules_given.get(child)?;
        let mut highest = 0;
        for rules in given {
            highest = highest.max(rules.budget_secs_on(day)?);
        }
        Some(highest)
    }

    /// The time that counts on all of a child's devices today, reported or
    /// not, less what was used beyond what a device held under an unlock,
    /// is within the highest budget in play plus the extra time.
    pub fn check_budget_of(&mut self, child: &str) {
        let now = self.now();
        // A child the guardian removed has nothing left to check.
        let Ok(today_view) = self.hub().child_today(child, now) else { return };
        let day = today_view.day;
        let Some(budget) = self.highest_budget(child, day) else { return };
        let mut used = today_view.used_secs;
        let mut extra = today_view.extra_secs;
        let mut over = 0;
        let mut parts = format!("master {used}, extra {extra}");
        if let Some(view) = self.engine.as_ref().unwrap().child_on_pc(child) {
            let unreported = view.unreported.iter().filter(|s| s.day == day && s.counts).map(|s| s.secs()).sum::<u64>();
            used += unreported;
            parts += &format!(", PC on day {} unreported {unreported} over {} decision {:?}", view.day, view.over_secs, view.decision);
        }
        let pc_beyond: u64 =
            self.pc_beyond.iter().filter(|((c, _, d), _)| c == child && *d == day).map(|(_, secs)| secs).sum();
        parts += &format!(", PC beyond {pc_beyond}");
        over += pc_beyond;
        for phone in self.phones.iter().filter(|p| p.child_id == child) {
            let unreported = phone.state.unconfirmed_slices().iter().filter(|s| s.day == day && s.counts).map(|s| s.secs()).sum::<u64>();
            used += unreported;
            let unsent = phone.state.unsent_added().iter().filter(|a| a.day == day).map(|a| a.secs).sum::<u64>();
            extra += unsent;
            let holding = phone.state.holding_at(now);
            let beyond = phone.beyond_by_day.get(&day).copied().unwrap_or(0);
            parts += &format!(", phone unreported {unreported} unsent added {unsent} beyond {beyond} holding {holding:?}");
            over += beyond;
        }
        if std::env::var("LIMO_AGENT_TRACE").is_ok() {
            println!("  call {} at {now}: {used} counted, {over} over, budget {budget}, extra {extra}: {parts}", self.calls);
        }
        assert!(
            used.saturating_sub(over) <= budget + extra,
            "call {}: {used} counted ({over} over under an unlock) on day {day}, with a budget of {budget} and {extra} extra: {parts}",
            self.calls
        );
    }
}
