//! The fake phone: a family's file with a real hub, a child with a phone
//! driven through the real engine (its own wall clock, elapsed count, start
//! count and facts, and a state file that holds only what the engine asked
//! to keep), and, when a story wants one, the child's device on the master
//! PC, driven with its own device state. Every call checks what must always
//! hold:
//!
//! - nothing is ever covered over a ringing call, the diallers, LiMo,
//!   Android's own lock screen or a screen that is off, and a cover during a
//!   call in progress offers the way back to it;
//! - the phone is covered by the lock exactly when the core says it is
//!   locked and the app in front is not one the lock leaves usable, once
//!   setup is finished; until then nothing is covered, counted or warned of;
//! - time is counted only while an app is in use and not covered;
//! - the phone's counted use never goes down within a day, and never grows
//!   by more than the elapsed count allows;
//! - the time that counts on all of the child's devices, reported or not,
//!   is within the highest budget in play plus the extra time.

use std::collections::BTreeMap;

use limo_core::{CivilDate, DeviceKind, DeviceState, GrantOutcome, RuleSet};
use limo_hub::{AgeBand, Hub};
use limo_phone::{
    CallState, Cover, Front, GuardScreen, GuardedPage, LockScreen, PhoneEngine, PhoneFacts, PhoneNote, PhoneOrders,
    PhonePermissions, PhoneView, WarningShown, app_in_use, may_cover,
};
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

/// A game: an ordinary app that counts.
pub const GAME: &str = "org.example.game";
/// A clock app the rules leave always allowed.
pub const CLOCK: &str = "org.example.clock";
/// An app the rules block.
pub const CASINO: &str = "org.example.casino";

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

/// The label of an app in front, as the facts give it.
fn facts_label(facts: &PhoneFacts) -> Option<String> {
    match &facts.front {
        Front::App { label, .. } => label.clone(),
        _ => None,
    }
}

/// The child's device on the master PC, driven with its own device state as
/// the agent would: it uses time only while its decision allows it, and
/// only what it holds.
pub struct FakePc {
    pub id: String,
    pub state: DeviceState,
    /// True while the child uses the PC.
    pub using: bool,
    last_report: i64,
    last_ask: i64,
    last_use: i64,
}

impl FakePc {
    fn exchange(&mut self, hub: &mut Hub, now: i64, want_more: bool) {
        hub.exchange_on_master_pc(&mut self.state, want_more, "", now).unwrap();
        if want_more {
            self.last_ask = now;
        }
        self.last_report = now;
    }

    /// Counted use on the PC today, reported or not.
    pub fn used_today(&self, now: i64) -> u64 {
        self.state.holding_at(now).used_secs()
    }

    /// True when the PC is locked now.
    pub fn locked(&self, now: i64) -> bool {
        self.state.clone().decide(now, Some("minecraft.exe")).device_lock.is_some()
    }
}

/// The family, its hub, and the child's phone.
pub struct Family {
    _dir: TempDir,
    hub: Hub,
    pub child: String,
    pub phone_id: String,
    /// The true time, unix milliseconds. The master PC's clock is right.
    true_ms: i64,
    engine: Option<PhoneEngine>,
    /// The phone's state file: only what the engine asked to keep.
    pub kept: Vec<u8>,
    /// How far the phone's wall clock is from the true time, in seconds.
    pub skew: i64,
    pub elapsed_ms: u64,
    pub boot: u32,
    pub offset: i32,
    pub hours_24: bool,
    pub auto_time: bool,
    pub screen_on: bool,
    pub keyguard: bool,
    pub call: CallState,
    pub front: Front,
    pub in_reach: bool,
    /// False while the link is up but the master does not answer.
    pub master_answers: bool,
    pub permissions: PhonePermissions,
    /// The page in front that the host says could switch LiMo off, if any.
    pub guarded: Option<GuardedPage>,
    /// The orders of the last call.
    pub orders: PhoneOrders,
    /// Every warning shown, with when, in order.
    pub warnings: Vec<(i64, WarningShown)>,
    /// Every note given, in order.
    pub notes: Vec<PhoneNote>,
    /// The child's device on the master PC, when a story pairs it.
    pub pc: Option<FakePc>,
    /// Check the budget at every call.
    pub check_budget: bool,
    pub calls: u64,
    pub restarts: u64,
    pub reports: u64,
    /// Reports the phone tried to send, answered or not.
    pub attempts: u64,
    rules_given: Vec<RuleSet>,
    /// The phone's day and counted use at the last call, and the elapsed
    /// count then, for the counting check.
    counted: Option<(i64, u64, u64)>,
    /// The phone's use beyond what it held, under an unlock, by day: the
    /// most seen.
    beyond: BTreeMap<i64, u64>,
}

impl Family {
    /// A family with one child, Mira, and her phone paired at home at
    /// `start` and set up (a guardian finished setup on it at once), screen
    /// on, unlocked, a game in front, out of reach.
    pub fn new(start: i64) -> Family {
        let mut f = Family::setting_up(start);
        assert_eq!(f.engine().finish_setup(start), Ok(true));
        assert!(f.engine().should_save());
        f.kept = f.engine().save();
        f
    }

    /// The same family with the phone just paired: setup not finished.
    pub fn setting_up(start: i64) -> Family {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = Hub::open(&dir.path().join("family.limo")).unwrap();
        hub.create_family(PASSWORD, start).unwrap();
        let child = hub.add_child("Mira", AgeBand::TenToTwelve, IST, start).unwrap().child_id;
        let rules = hub.rules(&child).unwrap();
        let pairing = hub.pair_device(&child, "Phone", DeviceKind::Portable, "android", start).unwrap();
        let mut engine = PhoneEngine::paired(&pairing.welcome, pairing.code_secret, start).unwrap();
        assert!(engine.should_save());
        let kept = engine.save();
        assert!(!engine.should_save());
        Family {
            _dir: dir,
            hub,
            child,
            phone_id: pairing.device_id,
            true_ms: start * 1_000,
            engine: Some(engine),
            kept,
            skew: 0,
            elapsed_ms: 3_600_000,
            boot: 7,
            offset: IST,
            hours_24: false,
            auto_time: true,
            screen_on: true,
            keyguard: false,
            call: CallState::Idle,
            front: Front::app(GAME),
            in_reach: false,
            master_answers: true,
            permissions: PhonePermissions::ALL,
            guarded: None,
            orders: PhoneOrders {
                cover: Cover::Nothing,
                time_left: limo_phone::TimeLeft {
                    seconds_left: None,
                    counting: false,
                    unlocked_until: None,
                    away: true,
                    locked: false,
                    words: String::new(),
                },
                warning: None,
                send_report: false,
                save_state: false,
                notes: Vec::new(),
                next_check_in_secs: 1,
            },
            warnings: Vec::new(),
            notes: Vec::new(),
            pc: None,
            check_budget: true,
            calls: 0,
            restarts: 0,
            reports: 0,
            attempts: 0,
            rules_given: vec![rules],
            counted: None,
            beyond: BTreeMap::new(),
        }
    }

    pub fn hub(&mut self) -> &mut Hub {
        &mut self.hub
    }

    pub fn engine(&mut self) -> &mut PhoneEngine {
        self.engine.as_mut().unwrap()
    }

    /// The true time, unix seconds: the master PC's clock.
    pub fn now(&self) -> i64 {
        self.true_ms.div_euclid(1_000)
    }

    /// What the phone's wall clock reads.
    pub fn wall(&self) -> i64 {
        self.now() + self.skew
    }

    /// Changes the child's rules at the master.
    pub fn change_rules(&mut self, change: impl FnOnce(&mut RuleSet)) -> RuleSet {
        let now = self.now();
        let mut rules = self.hub.rules(&self.child).unwrap();
        change(&mut rules);
        let taken = self.hub.replace_rules(&self.child, rules, now).unwrap();
        self.rules_given.push(taken.clone());
        taken
    }

    /// Changes the rules at the master, then brings the phone to them at
    /// home with its screen off (so that it takes no lease), as setup does,
    /// and leaves it as it was.
    pub fn set_rules(&mut self, change: impl FnOnce(&mut RuleSet)) {
        self.change_rules(change);
        let (away, screen_on) = (!self.in_reach, self.screen_on);
        self.in_reach = true;
        self.screen_on = false;
        self.tick();
        self.in_reach = !away;
        self.screen_on = screen_on;
        self.tick();
    }

    /// Pairs the child's device on the master PC.
    pub fn pair_pc(&mut self) {
        let now = self.now();
        let record = self.hub.pair_master_pc(&self.child, "Family PC", now).unwrap();
        self.pc = Some(FakePc {
            id: record.device_id,
            state: record.state,
            using: false,
            last_report: now,
            last_ask: i64::MIN / 2,
            last_use: now,
        });
    }

    /// Time passes, by `ms`, on the true clock and the elapsed count.
    pub fn advance(&mut self, ms: u64) {
        self.true_ms += ms as i64;
        self.elapsed_ms += ms;
    }

    /// The phone's process is killed and started again: the engine is
    /// loaded from the state file, with nothing kept on the way out.
    pub fn crash(&mut self) {
        drop(self.engine.take());
        let engine = PhoneEngine::load(&self.kept);
        assert_eq!(engine.load_problem(), None);
        self.engine = Some(engine);
        self.restarts += 1;
        // A crash may lose the last few seconds counted.
        self.counted = None;
    }

    /// The phone is turned off for `secs` and on again: a new start, an
    /// elapsed count from zero, and the engine loaded from the state file.
    pub fn reboot(&mut self, secs: i64) {
        self.true_ms += secs * 1_000;
        self.boot += 1;
        self.elapsed_ms = 25_000;
        self.crash();
    }

    /// The facts as they stand.
    pub fn facts(&self) -> PhoneFacts {
        let now = self.now();
        let prodded = self.in_reach && self.hub.wanted_reports(now).unwrap().contains(&self.phone_id);
        PhoneFacts {
            wall_secs: self.wall(),
            elapsed_ms: self.elapsed_ms,
            boot_count: self.boot,
            utc_offset_secs: self.offset,
            hours_24: self.hours_24,
            auto_time: self.auto_time,
            interactive: self.screen_on,
            keyguard_locked: self.keyguard,
            call: self.call,
            front: self.front.clone(),
            in_reach: self.in_reach,
            prodded,
            permissions: self.permissions,
            guarded: self.guarded,
        }
    }

    /// Keeps the state when the engine asks, as the host does.
    fn keep(&mut self) {
        let engine = self.engine.as_mut().unwrap();
        if engine.should_save() {
            self.kept = engine.save();
        }
    }

    /// One exchange with the master, as the host carries it.
    fn exchange(&mut self) {
        let wall = self.wall();
        let now = self.now();
        let report = self.engine.as_mut().unwrap().outgoing_report(wall).unwrap();
        self.keep();
        self.attempts += 1;
        if !self.master_answers {
            self.engine().lost_contact();
            self.keep();
            return;
        }
        let grant = self.hub.handle_report(&report, now).unwrap();
        let taken = self.engine().take_grant(&grant, wall);
        assert_ne!(taken.outcome, GrantOutcome::NotForThisDevice);
        self.keep();
        self.reports += 1;
        if taken.report_again {
            let report = self.engine().outgoing_report(wall).unwrap();
            self.keep();
            let grant = self.hub.handle_report(&report, now).unwrap();
            self.engine().take_grant(&grant, wall);
            self.keep();
            self.reports += 1;
        }
    }

    /// One call, as the host makes it, and the checks.
    pub fn tick(&mut self) -> &PhoneOrders {
        let facts = self.facts();
        let mut orders = self.engine().tick(&facts);
        if orders.save_state {
            self.keep();
        }
        if orders.send_report {
            self.exchange();
            // The host calls again at once, so that the orders follow the
            // grant.
            let again = PhoneFacts { prodded: false, ..facts.clone() };
            let more = self.engine().tick(&again);
            orders.notes.extend(more.notes.clone());
            orders = PhoneOrders { notes: orders.notes, warning: orders.warning.or(more.warning.clone()), ..more };
            if orders.save_state {
                self.keep();
            }
        }
        self.calls += 1;
        if let Some(w) = &orders.warning {
            self.warnings.push((facts.wall_secs, w.clone()));
        }
        self.notes.extend(orders.notes.clone());
        self.orders = orders;
        self.check_cover(&facts);
        self.check_counting();
        self.drive_pc();
        if self.check_budget {
            self.check_budget();
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

    /// Calls a second apart until the true clock reads `t`.
    pub fn run_until(&mut self, t: i64) {
        while self.now() < t {
            self.advance(1_000);
            self.tick();
        }
    }

    /// Calls a second apart until the phone is covered by the lock.
    pub fn run_until_locked(&mut self) {
        let start = self.now();
        while !matches!(self.orders.cover, Cover::Lock(_)) {
            assert!(self.now() - start < 30 * HOUR, "never locked");
            self.advance(1_000);
            self.tick();
        }
    }

    /// The lock covering the phone.
    pub fn lock(&self) -> LockScreen {
        match &self.orders.cover {
            Cover::Lock(screen) => (**screen).clone(),
            other => panic!("not locked: {other:?}"),
        }
    }

    /// The guard covering the phone.
    pub fn guard(&self) -> GuardScreen {
        match &self.orders.cover {
            Cover::Guard(screen) => (**screen).clone(),
            other => panic!("not guarded: {other:?}"),
        }
    }

    /// The engine's view of the phone.
    pub fn view(&self) -> PhoneView {
        self.engine.as_ref().unwrap().view().unwrap()
    }

    /// Counted use on the phone today, reported or not.
    pub fn counted(&self) -> u64 {
        self.view().counted_secs
    }

    /// Counted use today on all of the child's devices, as the master has it.
    pub fn master_used(&mut self) -> u64 {
        let now = self.now();
        self.hub.child_today(&self.child, now).unwrap().used_secs
    }

    /// A way out, as the lock's screen takes it, kept at once.
    pub fn way_out<T>(&mut self, act: impl FnOnce(&mut PhoneEngine, i64) -> T) -> T {
        let wall = self.wall();
        let out = act(self.engine.as_mut().unwrap(), wall);
        self.keep();
        out
    }

    /// Nothing is covered where it must not be; the lock covers exactly
    /// when the core says the phone is locked; time counts only while an app
    /// is in use and uncovered.
    fn check_cover(&self, facts: &PhoneFacts) {
        let orders = &self.orders;
        if !may_cover(facts) {
            assert_eq!(orders.cover, Cover::Nothing, "covered over {facts:?}");
        }
        if facts.call == CallState::Ringing || matches!(facts.front, Front::Dialler | Front::EmergencyDialler) {
            assert_eq!(orders.cover, Cover::Nothing, "covered over a call or a dialler");
        }
        // A cover during a call in progress offers the way back to it, and
        // only then.
        let call_on = facts.call == CallState::Ongoing;
        let back = call_on.then(|| "Back to the call".to_string());
        let view = self.view();
        // Until setup is finished nothing is covered, counted or warned of,
        // whatever the core says, and the time left says so.
        if !view.setup.finished {
            assert_eq!(orders.cover, Cover::Nothing, "covered during setup");
            assert!(!orders.time_left.counting && !orders.time_left.locked, "counted or locked during setup");
            assert_eq!(orders.warning, None, "a warning during setup");
            assert_eq!(orders.time_left.words, "Setup is not finished on this phone.");
            return;
        }
        let decision = view.decision.expect("a decision");
        match (&orders.cover, decision.device_lock) {
            (Cover::Lock(screen), Some(_)) => {
                assert!(!decision.allowed, "an allowed app covered: {facts:?}");
                assert_eq!(screen.allowed_packages, view.rules.always_allowed_apps);
                assert_eq!((screen.call_on, &screen.back_to_call), (call_on, &back));
            }
            (Cover::Lock(_), None) => panic!("covered, but the core says open"),
            (_, Some(_)) if may_cover(facts) && !decision.allowed => panic!("the core says locked: {orders:?}"),
            (Cover::Block { package, go_home, call_on: on, back_to_call, .. }, None) => {
                assert!(view.rules.blocked_apps.contains(package) && *go_home);
                assert_eq!(facts.front.clone(), Front::App { package: package.clone(), label: facts_label(facts) });
                assert_eq!((*on, back_to_call), (call_on, &back));
            }
            _ => {}
        }
        // The guard covers a guarded page exactly when nothing else does, a
        // right PIN does not keep such pages open, and the page is one to
        // guard (the device admin's only while the protection is on); it
        // offers the PIN only.
        let open = self.engine.as_ref().unwrap().guard_open_until().is_some();
        let to_guard = facts.guarded.is_some_and(|page| {
            page != GuardedPage::DeviceAdmin || facts.permissions.protection
        });
        let guard_due = to_guard && may_cover(facts) && decision.allowed && !open;
        match &orders.cover {
            Cover::Guard(screen) => {
                assert!(guard_due, "a guard where none is due: {facts:?}, open {open}");
                assert_eq!(facts.guarded, Some(screen.page));
                assert_eq!((screen.call_on, &screen.back_to_call), (call_on, &back));
                assert!(screen.ways.iter().all(|w| w.way == limo_phone::Way::Pin), "the guard offers the PIN only");
            }
            other => assert!(!guard_due, "a guarded page not guarded: {facts:?}, covered by {other:?}"),
        }
        if orders.time_left.counting {
            assert!(app_in_use(facts).is_some(), "counting with nothing in use");
            assert_eq!(orders.cover, Cover::Nothing, "counting under a cover");
        }
    }

    /// Counted use never goes down within a day, and never grows by more
    /// than the elapsed count allows.
    fn check_counting(&mut self) {
        let view = self.view();
        if let Some((day, before, elapsed)) = self.counted
            && day == view.day
        {
            let step = self.elapsed_ms.saturating_sub(elapsed);
            assert!(view.counted_secs >= before, "counted went down at call {}: {before} to {}", self.calls, view.counted_secs);
            assert!(
                view.counted_secs - before <= step.div_ceil(1_000),
                "call {} counted {} seconds in a step of {step} ms",
                self.calls,
                view.counted_secs - before
            );
        }
        let beyond = self.beyond.entry(view.day).or_default();
        *beyond = (*beyond).max(view.over_secs);
        self.counted = Some((view.day, view.counted_secs, self.elapsed_ms));
    }

    /// The PC, when paired, reports when the hub wants it to and every
    /// minute, and asks for time when it runs low while in use (again each
    /// minute while it is still low). In use, it spends what it holds.
    fn drive_pc(&mut self) {
        let now = self.now();
        let Some(mut pc) = self.pc.take() else { return };
        if pc.using {
            let from = pc.last_use.max(now - 10);
            if now > from {
                let d = pc.state.decide(from, Some("minecraft.exe"));
                if d.allowed {
                    let can = match (d.seconds_in_hand, d.unlocked_until) {
                        (Some(h), None) => (now - from).min(h as i64),
                        _ => now - from,
                    };
                    if can > 0 {
                        pc.state.record_use("minecraft.exe", from, from + can);
                    }
                }
            }
        }
        pc.last_use = now;
        let d = pc.state.decide(now, Some("minecraft.exe"));
        let ask = d.ask_master && pc.using && now - pc.last_ask >= limo_core::RETRY_SECS;
        let wanted = self.hub.wanted_reports(now).unwrap().contains(&pc.id);
        if wanted || now - pc.last_report >= 60 || ask {
            pc.exchange(&mut self.hub, now, ask);
        }
        self.pc = Some(pc);
    }

    /// The highest budget the child's rules gave for `day`, or None for no
    /// limit in some rules.
    fn highest_budget(&self, day: i64) -> Option<u64> {
        let mut highest = 0;
        for rules in &self.rules_given {
            highest = highest.max(rules.budget_secs_on(day)?);
        }
        Some(highest)
    }

    /// The time that counts on all of the child's devices today, reported or
    /// not, less what was used beyond what a device held under an unlock, is
    /// within the highest budget in play plus the extra time.
    pub fn check_budget(&mut self) {
        let now = self.now();
        let today = self.hub.child_today(&self.child, now).unwrap();
        let day = today.day;
        let Some(budget) = self.highest_budget(day) else { return };
        let view = self.view();
        let phone_unreported: u64 = view.unreported.iter().filter(|s| s.day == day && s.counts).map(|s| s.secs()).sum();
        let unsent: u64 = view.unsent_added.iter().filter(|a| a.day == day).map(|a| a.secs).sum();
        let mut used = today.used_secs + phone_unreported;
        let mut extra = today.extra_secs + unsent;
        if let Some(pc) = &self.pc {
            used += pc.state.unconfirmed_slices().iter().filter(|s| s.day == day && s.counts).map(|s| s.secs()).sum::<u64>();
            extra += pc.state.unsent_added().iter().filter(|a| a.day == day).map(|a| a.secs).sum::<u64>();
        }
        let over = self.beyond.get(&day).copied().unwrap_or(0);
        assert!(
            used.saturating_sub(over) <= budget + extra,
            "call {}: {used} counted ({over} over under an unlock) on day {day}, with a budget of {budget} and {extra} extra",
            self.calls
        );
    }
}
