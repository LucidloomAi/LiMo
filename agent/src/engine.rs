//! The [`Engine`]: called about once a second with the facts, it returns the
//! orders; called from a lock screen, it takes the ways out.

use std::collections::{BTreeMap, BTreeSet};

use limo_core::{
    Decision, DeviceKind, HomeCalendar, PinAction, Slice, TimeRequest, TryOutcome,
};
use limo_hub::{AccountStanding, Hub, PcSettings, UnlinkedAccounts};
use serde::{Deserialize, Serialize};

use crate::child::{ChildRun, Moment};
use crate::error::{EngineError, HubStep};
use crate::facts::{Facts, Program, Session, SessionState};
use crate::orders::{CloseOrder, LockScreen, LockWhy, Orders, SessionOrder, Show, WarningShown};
use crate::CLOCK_JUMP_MS;

/// Notes kept in memory before the set of them is cleared; the hub keeps
/// each note once in any case.
const MAX_NOTED: usize = 512;

/// The clock as the last call saw it.
#[derive(Debug, Clone, Copy)]
struct ClockSeen {
    wall: i64,
    steady_ms: u64,
    offset: i32,
}

/// What the hub said of a device the books want to hear from.
#[derive(Debug, Clone)]
struct DeviceSeen {
    kind: DeviceKind,
    child_id: String,
}

/// What a child's turn in a call gave, for the orders of its sessions.
struct ChildOutcome {
    device_id: String,
    decision: Option<Decision>,
    blocked: Vec<String>,
    counting: bool,
    latest_request: Option<limo_core::RequestView>,
}

/// One child's device on this PC, as the engine holds it now: for the
/// service's own view, and for tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildOnPc {
    /// The child.
    pub child_id: String,
    /// The child's device on this PC.
    pub device_id: String,
    /// The core's decision behind the last orders.
    pub decision: Option<Decision>,
    /// The home day of that decision.
    pub day: i64,
    /// Counted use on this PC on that day, reported or not, in seconds.
    pub counted_secs: u64,
    /// The part of it beyond everything the PC held that day: use while a
    /// guardian's unlock kept the PC open with nothing in hand. 0 otherwise.
    pub over_secs: u64,
    /// Whole seconds of use counted but not yet recorded in the device
    /// state.
    pub open_secs: u64,
    /// Use the master has not confirmed yet, the open stretch included.
    pub unreported: Vec<Slice>,
}

/// The engine of the agent on the master PC. It holds, for each child seen
/// on this PC, the child's device state and what it has counted, and keeps
/// them in the family's file through the [`Hub`] it is handed at every
/// call. It holds no clock and does nothing on its own.
pub struct Engine {
    pc_name: String,
    clock: Option<ClockSeen>,
    children: BTreeMap<String, ChildRun>,
    standings: BTreeMap<String, AccountStanding>,
    devices_seen: BTreeMap<String, DeviceSeen>,
    session_children: BTreeMap<u32, String>,
    noted: BTreeSet<String>,
    /// Problems met since the last orders were returned.
    problems: Vec<EngineError>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("pc_name", &self.pc_name)
            .field("children", &self.children.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// The child's session in use, if any: the one with the newest input when
/// there are several.
fn in_use<'a>(sessions: &[&'a Session]) -> Option<&'a Session> {
    sessions.iter().copied().filter(|s| s.state == SessionState::InUse).min_by_key(|s| s.idle_secs)
}

impl Engine {
    /// A new engine. `pc_name` is the name the guardian sees for each
    /// child's device on this PC, such as the PC's own name.
    pub fn new(pc_name: &str) -> Engine {
        Engine {
            pc_name: pc_name.to_string(),
            clock: None,
            children: BTreeMap::new(),
            standings: BTreeMap::new(),
            devices_seen: BTreeMap::new(),
            session_children: BTreeMap::new(),
            noted: BTreeSet::new(),
            problems: Vec::new(),
        }
    }

    /// One call: takes the facts, counts the time since the last call,
    /// exchanges with the children's books when due, and returns the orders.
    /// It never fails: when the family's file cannot be written, it goes on
    /// from what it holds and says so in [`Orders::problems`].
    pub fn tick(&mut self, hub: &mut Hub, facts: &Facts) -> Orders {
        let at = Moment { wall: facts.wall_secs, steady_ms: facts.steady_ms };
        // A device the hub said is gone is let go of now, and paired anew
        // below if its child is still on this PC.
        self.children.retain(|_, run| !run.gone);
        self.watch_clock(hub, facts);
        let settings = hub.pc_settings().unwrap_or_else(|problem| {
            self.problems.push(EngineError::hub(HubStep::ReadSettings, problem));
            PcSettings { idle_minutes: limo_hub::DEFAULT_IDLE_MINUTES, unlinked_accounts: UnlinkedAccounts::Free }
        });
        let idle_limit_ms = u64::from(settings.idle_minutes).saturating_mul(60_000);

        // Who is who, and which children are on this PC now.
        let standings: Vec<AccountStanding> =
            facts.sessions.iter().map(|s| self.standing(hub, &s.account_id)).collect();
        let mut in_play: Vec<String> = Vec::new();
        for standing in &standings {
            if let AccountStanding::Child { child_id } = standing
                && !in_play.contains(child_id)
            {
                in_play.push(child_id.clone());
            }
        }

        // The devices the books want to hear from: this PC's own are answered
        // here, the rest are for the service to prod.
        let wanted = hub.wanted_reports(at.wall).unwrap_or_else(|problem| {
            self.problems.push(EngineError::hub(HubStep::ReadWanted, problem));
            Vec::new()
        });
        let mut prod_devices = Vec::new();
        let mut wanted_children = BTreeSet::new();
        for device_id in wanted {
            match self.device_seen(hub, &device_id) {
                Some(seen) if seen.kind == DeviceKind::OnMaster => {
                    wanted_children.insert(seen.child_id);
                }
                _ => prod_devices.push(device_id),
            }
        }

        let mut outcomes: BTreeMap<String, ChildOutcome> = BTreeMap::new();
        for child_id in &in_play {
            let sessions: Vec<&Session> = facts
                .sessions
                .iter()
                .zip(&standings)
                .filter(|(_, standing)| matches!(standing, AccountStanding::Child { child_id: c } if c == child_id))
                .map(|(session, _)| session)
                .collect();
            let wanted = wanted_children.contains(child_id);
            let outcome = self.run_child(hub, child_id, &sessions, at, idle_limit_ms, wanted);
            outcomes.insert(child_id.clone(), outcome);
        }
        // Children no longer on this PC: what they used is reported, then
        // the engine lets go of them.
        let leaving: Vec<String> = self.children.keys().filter(|c| !in_play.contains(c)).cloned().collect();
        for child_id in leaving {
            let wanted = wanted_children.contains(&child_id);
            self.let_go(hub, &child_id, at, wanted);
            wanted_children.remove(&child_id);
        }
        // Devices on this PC the books want to hear from, of children not on
        // it now: one exchange each, to take the news.
        for child_id in wanted_children.iter().filter(|c| !in_play.contains(c)) {
            self.answer_wanted(hub, child_id, at);
        }

        // The orders, session by session.
        self.session_children.clear();
        let local_day = HomeCalendar::new(facts.utc_offset_secs, 0).day_of(facts.wall_secs);
        let mut sessions = Vec::new();
        for (session, standing) in facts.sessions.iter().zip(&standings) {
            let order = match standing {
                AccountStanding::NotLimited => nothing(session.session_id),
                AccountStanding::NotLinked => {
                    if session.state == SessionState::InUse {
                        self.note_unlinked(hub, session, local_day, at.wall);
                    }
                    // An administrator's account that is not linked is never
                    // locked, whatever the setting: locking it could lock
                    // out the guardian who has to set LiMo up.
                    match settings.unlinked_accounts {
                        _ if session.is_administrator => nothing(session.session_id),
                        UnlinkedAccounts::Free => nothing(session.session_id),
                        UnlinkedAccounts::Locked => SessionOrder {
                            session_id: session.session_id,
                            child_id: None,
                            helper_needed: true,
                            show: locked(LockWhy::AccountNotLinked, None, None, None),
                            close: Vec::new(),
                        },
                    }
                }
                AccountStanding::Child { child_id } => {
                    self.session_children.insert(session.session_id, child_id.clone());
                    match outcomes.get(child_id) {
                        Some(outcome) => self.child_order(hub, session, child_id, outcome, at.wall),
                        None => nothing(session.session_id),
                    }
                }
            };
            sessions.push(order);
        }
        Orders { sessions, prod_devices, problems: std::mem::take(&mut self.problems) }
    }

    /// Notes for the guardian when the wall clock moved against the steady
    /// count, or the time zone changed, since the last call.
    fn watch_clock(&mut self, hub: &mut Hub, facts: &Facts) {
        if let Some(prev) = self.clock {
            if let Some(step) = facts.steady_ms.checked_sub(prev.steady_ms) {
                let wall_ms = facts.wall_secs.saturating_sub(prev.wall).saturating_mul(1_000);
                let off = wall_ms.saturating_sub(i64::try_from(step).unwrap_or(i64::MAX));
                if off.unsigned_abs() > CLOCK_JUMP_MS {
                    let by_secs = if off >= 0 { off.saturating_add(500) / 1_000 } else { off.saturating_sub(500) / 1_000 };
                    if let Err(problem) = hub.note_clock_jump(by_secs, facts.wall_secs) {
                        self.problems.push(EngineError::hub(HubStep::Note, problem));
                    }
                }
            }
            if prev.offset != facts.utc_offset_secs
                && let Err(problem) = hub.note_time_zone_change(prev.offset, facts.utc_offset_secs, facts.wall_secs)
            {
                self.problems.push(EngineError::hub(HubStep::Note, problem));
            }
        }
        self.clock = Some(ClockSeen { wall: facts.wall_secs, steady_ms: facts.steady_ms, offset: facts.utc_offset_secs });
    }

    /// Which child an account is, as the hub says now, or as it last said
    /// when it cannot be read.
    fn standing(&mut self, hub: &Hub, account_id: &str) -> AccountStanding {
        match hub.account_standing(account_id) {
            Ok(standing) => {
                self.standings.insert(account_id.to_string(), standing.clone());
                standing
            }
            Err(problem) => {
                self.problems.push(EngineError::hub(HubStep::ReadAccount, problem));
                self.standings.get(account_id).cloned().unwrap_or(AccountStanding::NotLinked)
            }
        }
    }

    /// What kind a device the books want to hear from is, and whose.
    fn device_seen(&mut self, hub: &Hub, device_id: &str) -> Option<DeviceSeen> {
        if let Some(run) = self.children.values().find(|r| r.device_id == device_id) {
            return Some(DeviceSeen { kind: DeviceKind::OnMaster, child_id: run.child_id.clone() });
        }
        if let Some(seen) = self.devices_seen.get(device_id) {
            return Some(seen.clone());
        }
        let info = hub.device(device_id).ok()?;
        let seen = DeviceSeen { kind: info.kind, child_id: info.child_id };
        self.devices_seen.insert(device_id.to_string(), seen.clone());
        Some(seen)
    }

    /// The child's run, loading it from the family's file, or pairing the
    /// child's device on this PC the first time the child is seen here.
    fn load(&mut self, hub: &mut Hub, child_id: &str, now: i64, pair: bool) -> Result<bool, EngineError> {
        if self.children.contains_key(child_id) {
            return Ok(true);
        }
        let record = if pair {
            Some(hub.pair_master_pc(child_id, &self.pc_name, now))
        } else {
            hub.master_pc(child_id).transpose()
        };
        let Some(record) = record else { return Ok(false) };
        let record = record.map_err(|problem| EngineError::hub(HubStep::LoadChild, problem))?;
        let (run, problem) = ChildRun::from_record(record);
        self.problems.extend(problem);
        self.children.insert(child_id.to_string(), run);
        Ok(true)
    }

    /// A child's turn in a call.
    fn run_child(
        &mut self,
        hub: &mut Hub,
        child_id: &str,
        sessions: &[&Session],
        at: Moment,
        idle_limit_ms: u64,
        wanted: bool,
    ) -> ChildOutcome {
        let cannot_check = |device_id: String| ChildOutcome {
            device_id,
            decision: None,
            blocked: Vec::new(),
            counting: false,
            latest_request: None,
        };
        if let Err(problem) = self.load(hub, child_id, at.wall, true) {
            self.problems.push(problem);
        }
        let Some(run) = self.children.get_mut(child_id) else { return cannot_check(String::new()) };

        let active = in_use(sessions);
        run.take_step(at, active, idle_limit_ms);
        let present = active.is_some_and(|s| s.full_screen || u64::from(s.idle_secs) * 1_000 <= idle_limit_ms);
        let app: Option<String> = active.map(|s| s.in_front.as_ref().map(Program::app_id).unwrap_or_default());

        // An exchange, when one is due.
        let before = run.probe(at.wall, app.as_deref());
        let asking = before.ask_master && active.is_some();
        let revision = match hub.book_revision(child_id) {
            Ok(revision) => Some(revision),
            Err(problem) => {
                self.problems.push(EngineError::hub(HubStep::ReadRevision, problem));
                None
            }
        };
        if !run.quiet(at)
            && run.exchange_due(at, wanted, revision, asking, before.day)
            && let Err(problem) = run.exchange(hub, asking, at)
        {
            self.problems.push(problem);
        }

        // The decision behind the orders, and whether time is counted now.
        let decision = run.decide(at.wall, app.as_deref());
        let rules = run.state.rules();
        let front_blocked =
            active.and_then(|s| s.in_front.as_ref()).is_some_and(|p| p.listed_in(&rules.blocked_apps, true));
        let counting = present && decision.device_lock.is_none() && !front_blocked;
        let look = match (counting, active, app.as_deref()) {
            (true, Some(session), Some(app)) => Some((session.session_id, app)),
            _ => None,
        };
        run.set_look(at, look, &decision);
        if !run.quiet(at)
            && let Err(problem) = run.save_if_changed(hub, at)
        {
            self.problems.push(problem);
        }
        let counts_now = counting && app.as_deref().is_some_and(|a| run.counts(a));
        run.decision = Some(decision.clone());
        run.decided_at = at.wall;
        ChildOutcome {
            device_id: run.device_id.clone(),
            latest_request: run.state.requests_today(at.wall).pop(),
            decision: Some(decision),
            blocked: rules.blocked_apps,
            counting: counts_now,
        }
    }

    /// A child no longer on this PC: what was counted is recorded and
    /// reported, the state is kept, and the engine lets go of it. When the
    /// guardian removed the child or its device meanwhile, there is nothing
    /// to keep, and that is no problem.
    fn let_go(&mut self, hub: &mut Hub, child_id: &str, at: Moment, wanted: bool) {
        let Some(mut run) = self.children.remove(child_id) else { return };
        run.take_step(at, None, 0);
        if let Some(decision) = run.decision.clone() {
            run.set_look(at, None, &decision);
        }
        let mut problems = Vec::new();
        if wanted || run.has_unreported() {
            problems.extend(run.exchange(hub, false, at).err());
        }
        if !run.gone {
            problems.extend(run.save_if_changed(hub, at).err());
        }
        if !run.gone {
            self.problems.extend(problems);
        }
    }

    /// A child's device on this PC that a book wants to hear from while the
    /// child is not here: one exchange, to take the news.
    fn answer_wanted(&mut self, hub: &mut Hub, child_id: &str, at: Moment) {
        match self.load(hub, child_id, at.wall, false) {
            Ok(true) => {}
            Ok(false) => return,
            Err(problem) => {
                self.problems.push(problem);
                return;
            }
        }
        if let Some(mut run) = self.children.remove(child_id)
            && let Err(problem) = run.exchange(hub, false, at)
        {
            self.problems.push(problem);
        }
    }

    /// The order for one of a child's sessions.
    fn child_order(
        &mut self,
        hub: &mut Hub,
        session: &Session,
        child_id: &str,
        outcome: &ChildOutcome,
        now: i64,
    ) -> SessionOrder {
        let mut close: Vec<CloseOrder> = Vec::new();
        let mut programs: Vec<&Program> = session.running.iter().collect();
        programs.extend(session.in_front.as_ref());
        for program in programs {
            if program.listed_in(&outcome.blocked, true) && !close.iter().any(|c| &c.program == program) {
                close.push(CloseOrder::blocked(program));
            }
        }
        let show = match &outcome.decision {
            None => locked(LockWhy::CannotCheck, None, None, None),
            Some(decision) => {
                for order in &close {
                    let name = order.program.listed_as(&outcome.blocked, true).unwrap_or_else(|| order.program.app_id());
                    self.note_blocked(hub, &outcome.device_id, &name, decision.day, now);
                }
                match decision.device_lock {
                    Some(lock) => locked(
                        LockWhy::from_core(lock.reason),
                        lock.until,
                        Some(decision),
                        outcome.latest_request.clone(),
                    ),
                    None => Show::TimeLeft {
                        seconds_left: decision.seconds_left,
                        counting: outcome.counting && session.state == SessionState::InUse,
                        warning: decision.warning.map(|w| WarningShown::new(w.cause, w.minutes, w.lock_at, now)),
                        unlocked_until: decision.unlocked_until,
                    },
                }
            }
        };
        SessionOrder {
            session_id: session.session_id,
            child_id: Some(child_id.to_string()),
            helper_needed: true,
            show,
            close,
        }
    }

    /// Notes once a day that an account that is not linked was in use.
    fn note_unlinked(&mut self, hub: &mut Hub, session: &Session, day: i64, now: i64) {
        let key = format!("unlinked/{day}/{}", session.account_id);
        if self.noted.contains(&key) {
            return;
        }
        match hub.note_unlinked_account_used(&session.account_id, &session.account_name, day, now) {
            Ok(_) => self.remember(key),
            Err(problem) => self.problems.push(EngineError::hub(HubStep::Note, problem)),
        }
    }

    /// Notes once a day for each program that a blocked program was closed.
    fn note_blocked(
        &mut self,
        hub: &mut Hub,
        device_id: &str,
        program: &str,
        day: i64,
        now: i64,
    ) {
        let key = format!("blocked/{day}/{device_id}/{program}");
        if self.noted.contains(&key) {
            return;
        }
        match hub.note_blocked_program_closed(device_id, program, day, now) {
            Ok(_) => self.remember(key),
            Err(problem) => self.problems.push(EngineError::hub(HubStep::Note, problem)),
        }
    }

    fn remember(&mut self, key: String) {
        if self.noted.len() >= MAX_NOTED {
            self.noted.clear();
        }
        self.noted.insert(key);
    }

    /// The run of the child whose session this is.
    fn run_for_session(&mut self, session_id: u32) -> Result<&mut ChildRun, EngineError> {
        let child_id = self.session_children.get(&session_id).ok_or(EngineError::NotAChildSession { session_id })?;
        self.children.get_mut(child_id).ok_or(EngineError::NotAChildSession { session_id })
    }

    /// After a way out changed a child's state: the book hears of it at
    /// once, and the state is kept. When that cannot be done now, it is done
    /// at the next call, and the problem is given with its orders.
    fn after_way_out(&mut self, hub: &mut Hub, session_id: u32, now: i64) {
        let steady_ms = self.clock.map_or(0, |c| c.steady_ms);
        let Ok(run) = self.run_for_session(session_id) else { return };
        let at = Moment { wall: now, steady_ms };
        run.exchange_soon = true;
        let mut problems = Vec::new();
        if let Err(problem) = run.exchange(hub, false, at) {
            problems.push(problem);
            if let Err(problem) = run.save_if_changed(hub, at) {
                problems.push(problem);
            }
        }
        self.problems.extend(problems);
    }

    /// The child in session `session_id` asks for `minutes` more today, with
    /// a short reason or none. The request goes to the book at once (and
    /// with every exchange after, until the book has it). Refused, in the
    /// core's words, when it cannot be made now: one is already waiting,
    /// today has no limit, or too many were made today.
    pub fn ask_for_time(
        &mut self,
        hub: &mut Hub,
        session_id: u32,
        minutes: u32,
        reason: &str,
        now: i64,
    ) -> Result<TimeRequest, EngineError> {
        let run = self.run_for_session(session_id)?;
        run.flush();
        let request = run.state.ask_for_time(minutes, reason, now).map_err(|problem| EngineError::Refused { problem })?;
        self.after_way_out(hub, session_id, now);
        Ok(request)
    }

    /// A one time code typed on the lock screen of session `session_id`. A
    /// right code adds its minutes on this PC for today, once; a wrong one
    /// counts as a wrong guess, and after five the next try must wait. The
    /// state is kept at once, right or wrong, and the book hears of it.
    pub fn try_code(&mut self, hub: &mut Hub, session_id: u32, typed: &str, now: i64) -> Result<TryOutcome, EngineError> {
        let run = self.run_for_session(session_id)?;
        run.flush();
        let outcome = run.state.try_code(typed, now);
        self.after_way_out(hub, session_id, now);
        Ok(outcome)
    }

    /// A guardian types the PIN on the lock screen of session `session_id`
    /// to add minutes, to unlock until a time, or to confirm a release (the
    /// engine only says the PIN was right; what a release means on the
    /// master PC is the service's to decide). Wrong PINs count with wrong
    /// codes. The state is kept at once, right or wrong, and the book hears
    /// of it.
    pub fn try_pin(
        &mut self,
        hub: &mut Hub,
        session_id: u32,
        pin: &str,
        action: PinAction,
        now: i64,
    ) -> Result<TryOutcome, EngineError> {
        let run = self.run_for_session(session_id)?;
        run.flush();
        let outcome = run.state.try_pin(pin, action, now).map_err(|problem| EngineError::Refused { problem })?;
        self.after_way_out(hub, session_id, now);
        Ok(outcome)
    }

    /// A clean stop, before the service stops: everything counted for every
    /// child is kept in the family's file now. Without it, a stop loses at
    /// most the last few seconds of counting, and only when the service is
    /// not back within [`crate::MAX_COUNTED_STEP_MS`].
    pub fn stop(&mut self, hub: &mut Hub) -> Vec<EngineError> {
        let mut problems = std::mem::take(&mut self.problems);
        for run in self.children.values_mut() {
            if let Err(problem) = run.save_now(hub) {
                problems.push(problem);
            }
        }
        problems
    }

    /// A child's device on this PC as the engine holds it now, if the child
    /// was on this PC at the last call.
    pub fn child_on_pc(&self, child_id: &str) -> Option<ChildOnPc> {
        let run = self.children.get(child_id)?;
        let probe = run.probe_state();
        let day = run.decision.as_ref().map_or_else(|| probe.holding_at(run.decided_at).day, |d| d.day);
        let holding = probe.holding_at(run.decided_at);
        let (counted_secs, over_secs) =
            if holding.day == day { (holding.used_secs(), holding.over_secs()) } else { (0, 0) };
        Some(ChildOnPc {
            child_id: run.child_id.clone(),
            device_id: run.device_id.clone(),
            decision: run.decision.clone(),
            day,
            counted_secs,
            over_secs,
            open_secs: run.open_secs(),
            unreported: run.unreported(),
        })
    }

    /// The children on this PC at the last call.
    pub fn children_on_pc(&self) -> Vec<String> {
        self.children.keys().cloned().collect()
    }
}

/// An order to show nothing.
fn nothing(session_id: u32) -> SessionOrder {
    SessionOrder { session_id, child_id: None, helper_needed: false, show: Show::Nothing, close: Vec::new() }
}

/// A lock screen.
fn locked(
    why: LockWhy,
    until: Option<i64>,
    decision: Option<&Decision>,
    latest_request: Option<limo_core::RequestView>,
) -> Show {
    Show::Locked {
        screen: LockScreen {
            why,
            title: why.title().to_string(),
            message: why.message().to_string(),
            until,
            can_ask_for_time: decision.is_some_and(|d| d.can_ask_for_time),
            can_use_code: decision.is_some_and(|d| d.can_use_code),
            can_use_pin: decision.is_some_and(|d| d.can_use_pin),
            try_wait_until: decision.and_then(|d| d.try_wait_until),
            latest_request,
        },
    }
}
