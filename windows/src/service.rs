//! The service's work, with no Windows code: about once a second it gathers
//! the facts, calls the engine and carries out the orders; ten times a
//! second it looks after its helpers.
//!
//! Everything here is written against the [`Os`] trait, so the tests run it
//! against a fake PC. The engine and the hub live here, on the service's one
//! thread; nothing else touches them.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, HashSet};

use limo_agent::{CloseOrder, Engine, Facts, Orders, Program, Session, SessionState, Show, WarningShown};
use limo_core::{PinAction, TryOutcome};
use limo_hub::Hub;

use crate::about::about_line;
use crate::close::{never_close, same_path};
use crate::os::{HelperId, HelperStarted, Os, ProcessSeen, Received, SendProblem, SessionSeen};
use crate::programs::ProgramCache;
use crate::protocol::{Answer, ChannelProblem, FromHelper, PROTOCOL_VERSION, Secret, ToHelper, encode};
use crate::quiet::Quiet;
use crate::settings::TEST_CAP_SECS;

/// How often the service looks after its helpers, in milliseconds: a
/// helper that died is noticed within this.
pub const STEP_MS: u64 = 100;
/// How often the engine is called, in milliseconds.
pub const TICK_MS: u64 = 1_000;
/// A helper's report is used for this long, in milliseconds; after it, the
/// session counts as seen with no report: input just now, nothing in full
/// screen, nothing in front. A missing helper never gives free time.
pub const REPORT_FRESH_MS: u64 = 3_000;
/// A session ordered locked, in use, with no helper confirming the lock on
/// the screen for this long, in milliseconds, is disconnected.
pub const LOCK_CONFIRM_MS: u64 = 5_000;
/// A helper that has not said hello this long after it started is ended
/// and started again, in milliseconds.
pub const HELLO_WAIT_MS: u64 = 10_000;
/// What to show is handed over again this often even when nothing
/// changed, in milliseconds.
pub const RESHOW_MS: u64 = 5_000;
/// After a launch that failed, or a helper that lived less than
/// [`SHORT_LIFE_MS`], the next launch waits this long, in milliseconds.
pub const RETRY_LAUNCH_MS: u64 = 1_000;
/// A helper that ends sooner than this after it started, in milliseconds,
/// is started again only after [`RETRY_LAUNCH_MS`].
pub const SHORT_LIFE_MS: u64 = 2_000;
/// A helper no longer needed is told to quit, and ended this long after,
/// in milliseconds, if it has not.
pub const QUIT_GRACE_MS: u64 = 3_000;
/// The least time between two ways out from one helper, in milliseconds.
pub const WAY_OUT_SPACING_MS: u64 = 1_000;
/// How often the family's file is tidied, in milliseconds: once an hour.
pub const TIDY_EVERY_MS: u64 = 3_600_000;
/// How long a child's name is used before it is read again, in
/// milliseconds.
pub const NAME_REFRESH_MS: u64 = 60_000;
/// Something that goes on happening is logged at most this often, in
/// milliseconds: every ten minutes.
pub const QUIET_MS: u64 = 600_000;
/// Starting the helper in one session is logged at most this often, in
/// milliseconds.
pub const LAUNCH_QUIET_MS: u64 = 10_000;
/// The most notices of programs closed kept for a session whose helper is
/// not running yet.
pub const MAX_NOTICES_WAITING: usize = 4;
/// A gap between two looks longer than this by the steady count, or the
/// two clocks moving apart by more than this, is logged, in milliseconds.
pub const GAP_NOTE_MS: u64 = 5_000;

/// What the service knows of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    /// The name the guardian sees for each child's device on this PC.
    pub pc_name: String,
    /// The service's own process id.
    pub own_pid: u32,
    /// The full path of the service's own program file.
    pub own_exe: String,
    /// The Windows folder, such as `C:\Windows`.
    pub windows_dir: String,
}

/// What one session shows now, as the service holds it: for the tests and
/// for `limo status` later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// What is handed to the helper.
    pub show: Show,
    /// Its number.
    pub seq: u64,
    /// The warning held until a helper says it showed it.
    pub held_warning: Option<WarningShown>,
    /// The process id of the session's helper, if one runs.
    pub helper_pid: Option<u32>,
    /// True when the session's helper confirmed the lock is on the screen.
    pub lock_confirmed: bool,
    /// True when the test cap lifted the lock.
    pub lifted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Report {
    idle_ms: u64,
    front_pid: Option<u32>,
    full_screen: bool,
}

/// A running helper, in the session it was started in.
#[derive(Debug)]
struct Slot {
    id: HelperId,
    pid: u32,
    started: u64,
    greeted: bool,
    about_told: bool,
    report: Option<(u64, Report)>,
    sent_seq: Option<u64>,
    sent_at: u64,
    last_way_out: Option<u64>,
}

impl Slot {
    fn new(started: &HelperStarted, now: u64) -> Slot {
        Slot {
            id: started.id,
            pid: started.pid,
            started: now,
            greeted: false,
            about_told: false,
            report: None,
            sent_seq: None,
            sent_at: 0,
            last_way_out: None,
        }
    }
}

/// A helper told to quit, ended at its deadline if it has not.
#[derive(Debug)]
struct Ending {
    id: HelperId,
    pid: u32,
    deadline: u64,
}

/// What the service keeps for one session between calls.
#[derive(Debug)]
struct Keep {
    account_id: String,
    state: SessionState,
    child_id: Option<String>,
    child_name: Option<String>,
    helper_needed: bool,
    engine_show: Show,
    held: Option<WarningShown>,
    show: Show,
    shown_name: Option<String>,
    seq: u64,
    lock_from_seq: Option<u64>,
    confirmed_by: Option<HelperId>,
    unconfirmed_since: Option<u64>,
    lock_started: Option<u64>,
    lifted: bool,
    launch_after: u64,
    died_at: Option<u64>,
    notices: Vec<String>,
}

impl Keep {
    fn new(account_id: &str) -> Keep {
        Keep {
            account_id: account_id.to_string(),
            state: SessionState::Disconnected,
            child_id: None,
            child_name: None,
            helper_needed: false,
            engine_show: Show::Nothing,
            held: None,
            show: Show::Nothing,
            shown_name: None,
            seq: 0,
            lock_from_seq: None,
            confirmed_by: None,
            unconfirmed_since: None,
            lock_started: None,
            lifted: false,
            launch_after: 0,
            died_at: None,
            notices: Vec::new(),
        }
    }
}

/// A way out from a lock screen, as a helper sent it.
enum WayOut {
    Ask { minutes: u32, reason: String },
    Code(Secret),
    Pin(Secret, PinAction),
}

/// The service: the engine, the family's file, and its helpers.
pub struct Service<O: Os> {
    os: O,
    hub: Hub,
    engine: Engine,
    config: ServiceConfig,
    keeps: BTreeMap<u32, Keep>,
    slots: BTreeMap<u32, Slot>,
    ending: Vec<Ending>,
    programs: ProgramCache,
    processes: Vec<ProcessSeen>,
    names: BTreeMap<String, (Option<String>, u64)>,
    quiet: Quiet,
    launches: Quiet,
    next_seq: u64,
    last_tick: Option<u64>,
    last_tidy: Option<u64>,
    last_look: Option<(i64, u64)>,
    told_no_network: bool,
    facts: Option<Facts>,
}

impl<O: Os> std::fmt::Debug for Service<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("sessions", &self.keeps.keys().collect::<Vec<_>>())
            .field("helpers", &self.slots.values().map(|s| s.pid).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// Words for the answer to a way out, for the log. Never what was typed.
fn answer_words(answer: &Answer) -> String {
    match answer {
        Answer::Asked { request } => format!("asked for {} minutes; it waits for a guardian", request.minutes),
        Answer::Tried { outcome } => match outcome {
            TryOutcome::Added { added } => format!("right; {} minutes added", added.secs / 60),
            TryOutcome::Unlocked { until } => format!("right; unlocked until {until} (unix seconds)"),
            TryOutcome::ReleaseConfirmed => "right; a release confirmed".to_string(),
            TryOutcome::Wrong { wrong_guesses, .. } => format!("wrong ({wrong_guesses} wrong since the last right one)"),
            TryOutcome::MustWait { .. } => "not tried: a wait after wrong guesses is in force".to_string(),
            TryOutcome::Malformed => "not tried: not the right shape".to_string(),
            TryOutcome::AlreadyUsed => "not tried: that code was already used today".to_string(),
            TryOutcome::NotSet => "not tried: not set up on this PC".to_string(),
        },
        Answer::Refused { message } => format!("refused: {message}"),
    }
}

impl<O: Os> Service<O> {
    /// A service with the family's file open, about to make its first call.
    pub fn new(os: O, hub: Hub, config: ServiceConfig) -> Service<O> {
        let engine = Engine::new(&config.pc_name);
        Service {
            os,
            hub,
            engine,
            config,
            keeps: BTreeMap::new(),
            slots: BTreeMap::new(),
            ending: Vec::new(),
            programs: ProgramCache::new(),
            processes: Vec::new(),
            names: BTreeMap::new(),
            quiet: Quiet::new(QUIET_MS),
            launches: Quiet::new(LAUNCH_QUIET_MS),
            next_seq: 0,
            last_tick: None,
            last_tidy: None,
            last_look: None,
            told_no_network: false,
            facts: None,
        }
    }

    /// The operating system, as the service holds it.
    pub fn os(&mut self) -> &mut O {
        &mut self.os
    }

    /// The family's file, as the service holds it.
    pub fn hub(&mut self) -> &mut Hub {
        &mut self.hub
    }

    /// The engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The facts of the last call to the engine.
    pub fn last_facts(&self) -> Option<&Facts> {
        self.facts.as_ref()
    }

    /// How many version resources were read since the service started.
    pub fn files_read(&self) -> u64 {
        self.programs.files_read()
    }

    /// What a session shows now, if the service knows the session.
    pub fn session_view(&self, session_id: u32) -> Option<SessionView> {
        let keep = self.keeps.get(&session_id)?;
        let slot = self.slots.get(&session_id);
        Some(SessionView {
            show: keep.show.clone(),
            seq: keep.seq,
            held_warning: keep.held.clone(),
            helper_pid: slot.map(|s| s.pid),
            lock_confirmed: keep.confirmed_by.is_some() && keep.confirmed_by == slot.map(|s| s.id),
            lifted: keep.lifted,
        })
    }

    /// One step, every [`STEP_MS`]: what the helpers said, helpers that
    /// died, the engine when a second has passed, helpers started where
    /// needed, the fallback for a lock not confirmed, and what to show
    /// handed over.
    pub fn step(&mut self) {
        let now = self.os.steady_ms();
        self.take_messages(now);
        self.watch_helpers(now);
        if self.last_tick.is_none_or(|t| now.saturating_sub(t) >= TICK_MS) {
            self.last_tick = Some(now);
            self.tick(now);
        }
        self.keep_helpers(now);
        self.fallback(now);
        self.hand_over(now);
    }

    /// Before the service stops: the engine keeps everything it counted,
    /// and every helper is told to quit.
    pub fn stop(&mut self) {
        let now = self.os.steady_ms();
        for problem in self.engine.stop(&mut self.hub) {
            self.log_quiet(&format!("problem/{problem}"), &format!("At the stop, the engine met a problem: {problem}"), now);
        }
        let helpers: Vec<HelperId> = self.slots.values().map(|s| s.id).collect();
        for id in helpers {
            if let Ok(frame) = encode(&ToHelper::Quit) {
                let _ = self.os.send(id, frame);
            }
        }
    }

    /// After [`Service::stop`], once the helpers had a moment to quit: any
    /// still running is ended.
    pub fn finish(&mut self) {
        let mut ids: Vec<HelperId> = self.slots.values().map(|s| s.id).collect();
        ids.extend(self.ending.iter().map(|e| e.id));
        self.slots.clear();
        self.ending.clear();
        for id in ids {
            self.os.end_helper(id);
        }
    }

    fn log_quiet(&mut self, key: &str, text: &str, now: u64) {
        if let Some(line) = self.quiet.line(key, text, now) {
            self.os.log(&line);
        }
    }

    // ----------------------------------------------------------------------
    // The helpers' channels
    // ----------------------------------------------------------------------

    fn session_of(&self, id: HelperId) -> Option<u32> {
        self.slots.iter().find(|(_, slot)| slot.id == id).map(|(session, _)| *session)
    }

    fn take_messages(&mut self, now: u64) {
        for (id, received) in self.os.received() {
            let Some(session) = self.session_of(id) else { continue };
            match received {
                Received::Closed => self.drop_helper(session, now, None),
                Received::Broken(problem) => self.drop_helper(session, now, Some(problem)),
                Received::Message(message) => {
                    if let Err(problem) = self.on_message(session, message, now) {
                        self.drop_helper(session, now, Some(problem));
                    }
                }
            }
        }
    }

    /// Ends a session's helper and forgets it; it is started again when the
    /// session still needs one.
    fn drop_helper(&mut self, session: u32, now: u64, problem: Option<ChannelProblem>) {
        let Some(slot) = self.slots.remove(&session) else { return };
        self.os.end_helper(slot.id);
        let (key, text) = match problem {
            Some(problem) => (
                format!("broken/{session}"),
                format!(
                    "The helper in session {session} (pid {}) was closed, and is started again, because {problem}.",
                    slot.pid
                ),
            ),
            None => (format!("ended/{session}"), format!("The helper in session {session} (pid {}) ended.", slot.pid)),
        };
        self.log_quiet(&key, &text, now);
        if let Some(keep) = self.keeps.get_mut(&session) {
            keep.died_at = Some(now);
            let short = now.saturating_sub(slot.started) < SHORT_LIFE_MS;
            keep.launch_after = if short { now.saturating_add(RETRY_LAUNCH_MS) } else { now };
        }
    }

    fn on_message(&mut self, session: u32, message: FromHelper, now: u64) -> Result<(), ChannelProblem> {
        let unexpected = |what: &str| ChannelProblem::Unexpected { what: what.to_string() };
        let Some(slot) = self.slots.get_mut(&session) else { return Ok(()) };
        if !slot.greeted {
            return match message {
                FromHelper::Hello { protocol } if protocol == PROTOCOL_VERSION => {
                    slot.greeted = true;
                    Ok(())
                }
                FromHelper::Hello { protocol } => Err(ChannelProblem::WrongVersion { got: protocol }),
                _ => Err(unexpected("it sent a message before saying hello")),
            };
        }
        match message {
            FromHelper::Hello { .. } => Err(unexpected("it said hello twice")),
            FromHelper::About { uiaccess, integrity, band, process_sddl } => {
                if slot.about_told {
                    return Err(unexpected("it said twice what it got"));
                }
                slot.about_told = true;
                let line = about_line(session, slot.pid, uiaccess, integrity, band, process_sddl.as_deref());
                self.os.log(&line);
                Ok(())
            }
            FromHelper::Report { idle_ms, front_pid, full_screen } => {
                slot.report = Some((now, Report { idle_ms, front_pid, full_screen }));
                Ok(())
            }
            FromHelper::LockOnScreen { seq } => {
                if slot.sent_seq.is_none_or(|sent| seq > sent) {
                    return Err(unexpected("it said a lock was on the screen that it was never sent"));
                }
                let id = slot.id;
                if let Some(keep) = self.keeps.get_mut(&session)
                    && keep.lock_from_seq.is_some_and(|from| seq >= from)
                    && keep.confirmed_by != Some(id)
                {
                    keep.confirmed_by = Some(id);
                    keep.unconfirmed_since = None;
                    let text = format!("The helper in session {session} confirmed the lock is on the screen.");
                    self.log_quiet(&format!("confirmed/{session}"), &text, now);
                }
                Ok(())
            }
            FromHelper::WarningShown { cause, minutes, lock_at } => {
                if let Some(keep) = self.keeps.get_mut(&session)
                    && keep.held.as_ref().is_some_and(|w| w.cause == cause && w.minutes == minutes && w.lock_at == lock_at)
                {
                    keep.held = None;
                    self.refresh_show(session);
                }
                Ok(())
            }
            FromHelper::AskForTime { ask, minutes, reason } => {
                self.way_out(session, ask, WayOut::Ask { minutes, reason }, now);
                Ok(())
            }
            FromHelper::TryCode { ask, code } => {
                self.way_out(session, ask, WayOut::Code(code), now);
                Ok(())
            }
            FromHelper::TryPin { ask, pin, action } => {
                self.way_out(session, ask, WayOut::Pin(pin, action), now);
                Ok(())
            }
        }
    }

    /// A way out from the lock screen of `session`: the session is the one
    /// the helper was started in, never anything the message said. What was
    /// typed goes to the engine for this call only, and is wiped when the
    /// call ends; it is never logged.
    fn way_out(&mut self, session: u32, ask: u32, way: WayOut, now: u64) {
        let Some(slot) = self.slots.get_mut(&session) else { return };
        let id = slot.id;
        let too_soon = slot.last_way_out.is_some_and(|t| now.saturating_sub(t) < WAY_OUT_SPACING_MS);
        let answer = if too_soon {
            Answer::Refused { message: "One thing at a time: try again in a moment.".to_string() }
        } else {
            slot.last_way_out = Some(now);
            let wall = self.os.wall_secs();
            let (kind, answer) = match way {
                WayOut::Ask { minutes, reason } => (
                    "a request for more time",
                    match self.engine.ask_for_time(&mut self.hub, session, minutes, &reason, wall) {
                        Ok(request) => Answer::Asked { request },
                        Err(problem) => Answer::Refused { message: problem.to_string() },
                    },
                ),
                WayOut::Code(code) => (
                    "a one time code",
                    match self.engine.try_code(&mut self.hub, session, code.expose(), wall) {
                        Ok(outcome) => Answer::Tried { outcome },
                        Err(problem) => Answer::Refused { message: problem.to_string() },
                    },
                ),
                WayOut::Pin(pin, action) => (
                    "the guardian PIN",
                    match self.engine.try_pin(&mut self.hub, session, pin.expose(), action, wall) {
                        Ok(outcome) => Answer::Tried { outcome },
                        Err(problem) => Answer::Refused { message: problem.to_string() },
                    },
                ),
            };
            self.os.log(&format!("Session {session}: {kind} was tried from the lock screen: {}.", answer_words(&answer)));
            if matches!(answer, Answer::Tried { outcome: TryOutcome::ReleaseConfirmed }) {
                self.os.log(
                    "A guardian confirmed a release with the PIN. Releasing the master PC is not built yet, so nothing more happens.",
                );
            }
            // The change shows at once, not at the next second.
            self.last_tick = None;
            answer
        };
        self.send(session, id, &ToHelper::Answer { ask, answer }, now);
    }

    /// Hands a message to a session's helper. A helper that does not read
    /// what it is sent is ended and started again.
    fn send(&mut self, session: u32, id: HelperId, message: &ToHelper, now: u64) -> bool {
        let frame = match encode(message) {
            Ok(frame) => frame,
            Err(problem) => {
                self.log_quiet("encode", &format!("A message to a helper could not be written: {problem}."), now);
                return false;
            }
        };
        match self.os.send(id, frame) {
            Ok(()) => true,
            Err(SendProblem::NotReading) => {
                let problem = ChannelProblem::Unexpected { what: "it stopped reading what it is sent".to_string() };
                self.drop_helper(session, now, Some(problem));
                false
            }
            Err(SendProblem::Gone) => {
                self.drop_helper(session, now, None);
                false
            }
        }
    }

    fn watch_helpers(&mut self, now: u64) {
        let sessions: Vec<u32> = self.slots.keys().copied().collect();
        for session in sessions {
            let Some(slot) = self.slots.get(&session) else { continue };
            let (id, greeted, started) = (slot.id, slot.greeted, slot.started);
            if !self.os.helper_running(id) {
                self.drop_helper(session, now, None);
            } else if !greeted && now.saturating_sub(started) >= HELLO_WAIT_MS {
                let problem = ChannelProblem::Unexpected { what: "it did not say hello within 10 seconds".to_string() };
                self.drop_helper(session, now, Some(problem));
            }
        }
        let mut still = Vec::new();
        for ending in std::mem::take(&mut self.ending) {
            if !self.os.helper_running(ending.id) {
                self.os.end_helper(ending.id);
            } else if now >= ending.deadline {
                self.os.end_helper(ending.id);
                self.os.log(&format!("The helper with pid {} did not quit when told, so it was ended.", ending.pid));
            } else {
                still.push(ending);
            }
        }
        self.ending = still;
    }

    // ----------------------------------------------------------------------
    // The engine, once a second
    // ----------------------------------------------------------------------

    fn is_own(&self, process: &ProcessSeen) -> bool {
        process.pid == self.config.own_pid
            || self.slots.values().any(|s| s.pid == process.pid)
            || self.ending.iter().any(|e| e.pid == process.pid)
            || same_path(&process.path, &self.config.own_exe)
    }

    fn note_gap(&mut self, wall: i64, now: u64) {
        if let Some((wall_before, steady_before)) = self.last_look {
            let steady = now.saturating_sub(steady_before);
            let wall_ms = wall.saturating_sub(wall_before).saturating_mul(1_000);
            let apart = wall_ms.saturating_sub(i64::try_from(steady).unwrap_or(i64::MAX)).unsigned_abs();
            if steady > GAP_NOTE_MS || apart > GAP_NOTE_MS {
                self.os.log(&format!(
                    "Since the last look, {} s passed by the steady count and {} s by the wall clock. \
                     Through a sleep both move alike; a clock that was set moves them apart.",
                    steady / 1_000,
                    wall.saturating_sub(wall_before)
                ));
            }
        }
        self.last_look = Some((wall, now));
    }

    /// The program in front of `session`, from the process id its helper
    /// named: only a process in that same session, never LiMo's own.
    fn front_program(&mut self, session: u32, pid: u32, processes: &mut Vec<ProcessSeen>, now: u64) -> Option<Program> {
        let process = match processes.iter().find(|p| p.pid == pid) {
            Some(process) => process.clone(),
            None => {
                let process = self.os.process(pid)?;
                if process.session_id != 0 {
                    processes.push(process.clone());
                }
                process
            }
        };
        if process.session_id != session {
            let text = format!(
                "The helper in session {session} named a process of another session as the one in front; it counts as nothing in front."
            );
            self.log_quiet(&format!("front/{session}"), &text, now);
            return None;
        }
        if self.is_own(&process) {
            return None;
        }
        Some(self.programs.program(&mut self.os, &process))
    }

    /// Idle seconds, full screen and the program in front, from the
    /// session's helper's fresh report; with none: input just now, nothing
    /// in full screen, nothing in front, so time counts.
    fn helper_sees(&mut self, session: u32, processes: &mut Vec<ProcessSeen>, now: u64) -> (u32, bool, Option<Program>) {
        let report = self
            .slots
            .get(&session)
            .and_then(|slot| slot.report)
            .filter(|(at, _)| now.saturating_sub(*at) <= REPORT_FRESH_MS)
            .map(|(_, report)| report);
        let Some(report) = report else { return (0, false, None) };
        let idle_secs = u32::try_from(report.idle_ms / 1_000).unwrap_or(u32::MAX);
        let in_front = report.front_pid.and_then(|pid| self.front_program(session, pid, processes, now));
        (idle_secs, report.full_screen, in_front)
    }

    fn tick(&mut self, now: u64) {
        let wall = self.os.wall_secs();
        let utc_offset_secs = self.os.utc_offset_secs();
        self.note_gap(wall, now);
        let mut seen: Vec<SessionSeen> = Vec::new();
        for session in self.os.sessions() {
            if session.session_id != 0 && !seen.iter().any(|s| s.session_id == session.session_id) {
                seen.push(session);
            }
        }
        let mut processes: Vec<ProcessSeen> = self.os.processes().into_iter().filter(|p| p.session_id != 0).collect();

        let mut sessions = Vec::with_capacity(seen.len());
        for s in &seen {
            let (idle_secs, full_screen, in_front) = self.helper_sees(s.session_id, &mut processes, now);
            sessions.push(Session {
                session_id: s.session_id,
                account_id: s.account_id.clone(),
                account_name: s.account_name.clone(),
                state: s.state,
                is_administrator: s.is_administrator,
                idle_secs,
                full_screen,
                in_front,
                running: Vec::new(),
            });
        }
        self.programs.keep_only(&processes);
        let mut running: BTreeMap<u32, HashSet<Program>> = BTreeMap::new();
        for process in &processes {
            if self.is_own(process) || !sessions.iter().any(|s| s.session_id == process.session_id) {
                continue;
            }
            let program = self.programs.program(&mut self.os, process);
            running.entry(process.session_id).or_default().insert(program);
        }
        for session in &mut sessions {
            let mut programs: Vec<Program> = running.remove(&session.session_id).unwrap_or_default().into_iter().collect();
            programs.sort_by(|a, b| a.file_name.cmp(&b.file_name).then(a.original_name.cmp(&b.original_name)));
            session.running = programs;
        }

        let facts = Facts { wall_secs: wall, steady_ms: now, utc_offset_secs, sessions };
        let orders = self.engine.tick(&mut self.hub, &facts);
        self.processes = processes;
        self.apply(orders, &facts, &seen, now, wall);
        self.facts = Some(facts);

        if self.last_tidy.is_none_or(|t| now.saturating_sub(t) >= TIDY_EVERY_MS) {
            self.last_tidy = Some(now);
            if let Err(problem) = self.hub.tidy(wall) {
                self.log_quiet("tidy", &format!("Tidying the family's file failed: {problem}"), now);
            }
        }
    }

    fn apply(&mut self, orders: Orders, facts: &Facts, seen: &[SessionSeen], now: u64, wall: i64) {
        for problem in &orders.problems {
            self.log_quiet(&format!("problem/{problem}"), &format!("The engine met a problem: {problem}"), now);
        }
        if !orders.prod_devices.is_empty() && !self.told_no_network {
            self.told_no_network = true;
            self.os.log(
                "The family's books asked to hear from a phone. LiMo for Windows has no network yet, so this is ignored (said once).",
            );
        }
        let test_cap = self.os.test_cap();
        let present: BTreeSet<u32> = orders.sessions.iter().map(|o| o.session_id).collect();

        for (order, session) in orders.sessions.into_iter().zip(&facts.sessions) {
            let id = order.session_id;
            let account_id = seen.iter().find(|s| s.session_id == id).map_or("", |s| s.account_id.as_str());
            // Another account in a session id seen before: start afresh.
            if self.keeps.get(&id).is_some_and(|k| k.account_id != account_id) {
                self.keeps.remove(&id);
                if let Some(slot) = self.slots.remove(&id) {
                    self.os.end_helper(slot.id);
                }
            }
            let mut lines = Vec::new();
            {
                let keep = self.keeps.entry(id).or_insert_with(|| Keep::new(account_id));
                keep.state = session.state;
                keep.helper_needed = order.helper_needed;
                keep.child_id = order.child_id.clone();
                match &order.show {
                    Show::TimeLeft { warning: Some(warning), .. } => keep.held = Some(warning.clone()),
                    Show::TimeLeft { .. } => {}
                    _ => keep.held = None,
                }
                if keep.held.as_ref().is_some_and(|w| wall >= w.lock_at) {
                    keep.held = None;
                }
                if matches!(order.show, Show::Locked { .. }) {
                    let started = *keep.lock_started.get_or_insert(now);
                    if test_cap && !keep.lifted && now.saturating_sub(started) >= TEST_CAP_SECS * 1_000 {
                        keep.lifted = true;
                        lines.push(format!(
                            "TEST CAP: the lock in session {id} has been on for {TEST_CAP_SECS} seconds, so the test cap lifted it. \
                             This is a test install (limo install --dev). A real install has no test cap."
                        ));
                    }
                } else {
                    keep.lock_started = None;
                    keep.lifted = false;
                }
                if keep.lifted && !test_cap {
                    keep.lifted = false;
                    lines.push(format!("The test cap was switched off, so the lock in session {id} is back."));
                }
                keep.engine_show = order.show;
            }
            for line in lines {
                self.os.log(&line);
            }
            self.refresh_name(id, now);
            self.refresh_show(id);
            if !order.close.is_empty() {
                self.close(id, &order.close, now);
            }
        }

        let gone: Vec<u32> = self.keeps.keys().filter(|id| !present.contains(id)).copied().collect();
        for id in gone {
            self.keeps.remove(&id);
            if let Some(slot) = self.slots.remove(&id) {
                self.os.end_helper(slot.id);
                self.os.log(&format!("Session {id} has ended, so its helper (pid {}) is closed.", slot.pid));
            }
        }
    }

    fn refresh_name(&mut self, session: u32, now: u64) {
        let Some(child_id) = self.keeps.get(&session).and_then(|k| k.child_id.clone()) else {
            if let Some(keep) = self.keeps.get_mut(&session) {
                keep.child_name = None;
            }
            return;
        };
        let fresh = self
            .names
            .get(&child_id)
            .filter(|(_, at)| now.saturating_sub(*at) < NAME_REFRESH_MS)
            .map(|(name, _)| name.clone());
        let name = match fresh {
            Some(name) => name,
            None => {
                let name = self.hub.child(&child_id).ok().map(|child| child.name);
                if self.names.len() > 64 {
                    self.names.clear();
                }
                self.names.insert(child_id, (name.clone(), now));
                name
            }
        };
        if let Some(keep) = self.keeps.get_mut(&session) {
            keep.child_name = name;
        }
    }

    /// What the session's helper shows: the engine's order, with the warning
    /// held until a helper says it showed it, or nothing while the test cap
    /// has lifted a lock. A change gives it a new number, and starts or ends
    /// the watch on a lock.
    fn refresh_show(&mut self, session: u32) {
        let Some(keep) = self.keeps.get_mut(&session) else { return };
        let mut show = if keep.lifted { Show::Nothing } else { keep.engine_show.clone() };
        if let Show::TimeLeft { warning, .. } = &mut show
            && warning.is_none()
        {
            warning.clone_from(&keep.held);
        }
        if keep.show == show && keep.shown_name == keep.child_name {
            return;
        }
        self.next_seq += 1;
        keep.seq = self.next_seq;
        keep.show = show;
        keep.shown_name = keep.child_name.clone();
        let is_lock = matches!(keep.show, Show::Locked { .. });
        match (is_lock, keep.lock_from_seq) {
            (true, None) => {
                keep.lock_from_seq = Some(keep.seq);
                keep.confirmed_by = None;
                keep.unconfirmed_since = None;
            }
            (false, Some(_)) => {
                keep.lock_from_seq = None;
                keep.confirmed_by = None;
                keep.unconfirmed_since = None;
                self.quiet.forget(&format!("confirmed/{session}"));
            }
            _ => {}
        }
    }

    /// Closes every running copy of each program in `closes`, in `session`
    /// only, then tells the session's helper why.
    fn close(&mut self, session: u32, closes: &[CloseOrder], now: u64) {
        if session == 0 {
            return;
        }
        for order in closes {
            let name = order.program.file_name.clone();
            let candidates: Vec<ProcessSeen> =
                self.processes.iter().filter(|p| p.session_id == session && p.session_id != 0).cloned().collect();
            let mut closed = 0;
            for process in candidates {
                if self.is_own(&process) {
                    continue;
                }
                if self.programs.program(&mut self.os, &process) != order.program {
                    continue;
                }
                if process.path.is_empty() {
                    let text = format!(
                        "The family's rules block {name} (pid {}) in session {session}, but LiMo cannot read where its file is, so it does not close it.",
                        process.pid
                    );
                    self.log_quiet(&format!("nopath/{session}/{name}"), &text, now);
                    continue;
                }
                if let Some(spared) = never_close(&process.path, &self.config.windows_dir) {
                    let text = format!(
                        "The family's rules block {name} in session {session}, but it is part of Windows and LiMo never closes it: {}",
                        spared.why
                    );
                    self.log_quiet(&format!("spared/{session}/{name}"), &text, now);
                    continue;
                }
                match self.os.end_process(&process) {
                    Ok(()) => closed += 1,
                    Err(problem) => {
                        let text = format!("LiMo could not close {name} (pid {}) in session {session}: {problem}", process.pid);
                        self.log_quiet(&format!("cannot-close/{session}/{name}"), &text, now);
                    }
                }
            }
            if closed > 0 {
                let copies = if closed == 1 { "1 running copy".to_string() } else { format!("{closed} running copies") };
                let text = format!("Closed {name} in session {session} ({copies}): {}", order.reason);
                self.log_quiet(&format!("closed/{session}/{}", order.program.app_id()), &text, now);
                // Handed to the helper with what to show, or as soon as one
                // runs.
                if let Some(keep) = self.keeps.get_mut(&session)
                    && !keep.notices.contains(&order.reason)
                {
                    if keep.notices.len() >= MAX_NOTICES_WAITING {
                        keep.notices.remove(0);
                    }
                    keep.notices.push(order.reason.clone());
                }
            }
        }
    }

    // ----------------------------------------------------------------------
    // Helpers started and ended, the fallback, and what to show
    // ----------------------------------------------------------------------

    fn keep_helpers(&mut self, now: u64) {
        let sessions: Vec<u32> = self.keeps.keys().copied().collect();
        for session in sessions {
            let Some(keep) = self.keeps.get(&session) else { continue };
            let (needed, after) = (keep.helper_needed, keep.launch_after);
            let running = self.slots.contains_key(&session);
            if needed && !running && now >= after {
                self.launch(session, now);
            } else if !needed && running && let Some(slot) = self.slots.remove(&session) {
                if let Ok(frame) = encode(&ToHelper::Quit) {
                    let _ = self.os.send(slot.id, frame);
                }
                self.ending.push(Ending { id: slot.id, pid: slot.pid, deadline: now.saturating_add(QUIT_GRACE_MS) });
            }
        }
    }

    fn launch(&mut self, session: u32, now: u64) {
        match self.os.start_helper(session) {
            Ok(started) => {
                let gap = self.keeps.get_mut(&session).and_then(|keep| keep.died_at.take()).map(|t| now.saturating_sub(t));
                let mut text = format!("Started the helper in session {session}: pid={} {}", started.pid, started.detail);
                if let Some(gap) = gap {
                    text.push_str(&format!(" gap_ms={gap} (its end was noticed within one {STEP_MS} ms step)"));
                }
                if let Some(line) = self.launches.line(&format!("launch/{session}"), &text, now) {
                    self.os.log(&line);
                }
                for problem in &started.problems {
                    self.log_quiet(&format!("launch-problem/{problem}"), problem, now);
                }
                self.quiet.forget(&format!("launch-failed/{session}"));
                self.slots.insert(session, Slot::new(&started, now));
            }
            Err(problem) => {
                let text = format!("The helper could not be started in session {session}: {problem}. LiMo tries again every second.");
                self.log_quiet(&format!("launch-failed/{session}"), &text, now);
                if let Some(keep) = self.keeps.get_mut(&session) {
                    keep.launch_after = now.saturating_add(RETRY_LAUNCH_MS);
                }
            }
        }
    }

    /// A session ordered locked and in use, with no helper confirming the
    /// lock on the screen within [`LOCK_CONFIRM_MS`], is disconnected, so
    /// that the Windows sign in screen covers it. Its programs stay open.
    fn fallback(&mut self, now: u64) {
        let sessions: Vec<u32> = self.keeps.keys().copied().collect();
        for session in sessions {
            let current = self.slots.get(&session).map(|s| s.id);
            let Some(keep) = self.keeps.get_mut(&session) else { continue };
            let locked = keep.lock_from_seq.is_some();
            let confirmed = keep.confirmed_by.is_some() && keep.confirmed_by == current;
            if !(locked && keep.state == SessionState::InUse && !confirmed) {
                keep.unconfirmed_since = None;
                continue;
            }
            let since = *keep.unconfirmed_since.get_or_insert(now);
            if now.saturating_sub(since) < LOCK_CONFIRM_MS {
                continue;
            }
            keep.unconfirmed_since = None;
            keep.state = SessionState::Disconnected;
            match self.os.disconnect_session(session) {
                Ok(()) => self.os.log(&format!(
                    "No helper confirmed the lock on the screen of session {session} within {} seconds, so LiMo disconnected the session: \
                     the Windows sign in screen covers it, and its programs stay open.",
                    LOCK_CONFIRM_MS / 1_000
                )),
                Err(problem) => {
                    let text = format!("LiMo could not disconnect session {session}, whose lock no helper confirmed: {problem}");
                    self.log_quiet(&format!("disconnect/{session}"), &text, now);
                }
            }
        }
    }

    fn hand_over(&mut self, now: u64) {
        let sessions: Vec<u32> = self.slots.keys().copied().collect();
        for session in sessions {
            let (Some(keep), Some(slot)) = (self.keeps.get(&session), self.slots.get(&session)) else { continue };
            if slot.sent_seq == Some(keep.seq) && now.saturating_sub(slot.sent_at) < RESHOW_MS {
                continue;
            }
            let id = slot.id;
            let seq = keep.seq;
            let message = ToHelper::Show { seq, child_name: keep.shown_name.clone(), show: keep.show.clone() };
            if self.send(session, id, &message, now)
                && let Some(slot) = self.slots.get_mut(&session)
            {
                slot.sent_seq = Some(seq);
                slot.sent_at = now;
            }
        }
        // Notices of programs closed, to the session's helper.
        let waiting: Vec<(u32, HelperId, Vec<String>)> = self
            .keeps
            .iter_mut()
            .filter(|(_, keep)| !keep.notices.is_empty())
            .filter_map(|(session, keep)| self.slots.get(session).map(|slot| (*session, slot.id, std::mem::take(&mut keep.notices))))
            .collect();
        for (session, id, notices) in waiting {
            for text in notices {
                if !self.send(session, id, &ToHelper::Notice { text }, now) {
                    break;
                }
            }
        }
    }
}
