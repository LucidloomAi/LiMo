//! One child's device on the master PC, as the engine runs it: the core's
//! device state, the stretch of use not yet recorded in it, the last look at
//! which the child's time was being counted, and when the device last
//! exchanged with the child's book.
//!
//! **How time is counted.** Between two calls the engine measures the step
//! with the steady count, never with the wall clock. A step counts when the
//! child's time was being counted at the look before it (the session in
//! use, input within the idle minutes or something in full screen, the PC
//! not locked, the program in front not blocked) and the same session is in
//! use now. It is cut where the idle minutes ran out, and never more than
//! the time the device held at the look before. A step longer than
//! [`crate::MAX_COUNTED_STEP_MS`] counts nothing: the PC slept, or the
//! service was held up.
//!
//! **Why a stretch.** Counted milliseconds gather in a stretch for the
//! program in front and go into the device state as whole seconds, as one
//! slice, at a flush: before every exchange, when the program changes, when
//! counting stops, when the clock jumps, and when a warning falls due. So a
//! slice is a minute or so of use, not a second. Decisions are made every
//! call on a copy of the state with the stretch recorded, so they are
//! exactly what they would be had every second been recorded. The stretch,
//! the look and the state are kept in the family's file at every call that
//! changes them, together: at once when anything but the counting itself
//! changed, and every few seconds while counting just goes on. A restart
//! then counts the use since they were kept as one longer step from the
//! kept look, so it loses nothing when the service is back within
//! [`crate::MAX_COUNTED_STEP_MS`], and never counts anything twice.

use limo_core::{Decision, DeviceState, Slice};
use limo_hub::{Hub, HubError, MasterPcRecord};
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, HubStep};
use crate::facts::Session;
use crate::{CLOCK_JUMP_MS, MAX_COUNTED_STEP_MS, REPORT_EVERY_SECS};

/// While locked for lack of time, or when the lease runs low, the device
/// asks again this often: the core's own retry, in milliseconds.
const ASK_AGAIN_MS: u64 = limo_core::RETRY_SECS as u64 * 1_000;
/// While a child's time is counted and nothing else changes, the stretch
/// and the look are kept at most this often, in milliseconds. They are
/// always kept together, so a restart counts the use since they were kept
/// as one longer step from the look, as long as that step is within
/// [`crate::MAX_COUNTED_STEP_MS`]. Anything else that changes is kept at
/// once.
const SAVE_PROGRESS_EVERY_MS: u64 = 5_000;
/// After the family's file failed a write, the engine waits this long
/// before writing for this child again, in milliseconds, so that a full disk
/// is not hammered every second. Ways out still try at once.
const QUIET_AFTER_FAILURE_MS: u64 = 5_000;

/// A moment: the wall clock and the steady count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Moment {
    /// Unix seconds.
    pub(crate) wall: i64,
    /// The steady count, milliseconds.
    pub(crate) steady_ms: u64,
}

/// Use of one program, measured by the steady count, not yet recorded in
/// the device state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stretch {
    /// The program, as LiMo names it.
    app: String,
    /// Milliseconds of use not yet recorded. A flush records the whole
    /// seconds; the part of a second stays here for more use of the same
    /// program.
    ms: u64,
    /// The wall clock at the last step counted: what a flush records ends
    /// here.
    wall_end: i64,
}

/// The last look at which the child's time was being counted. The next
/// step is counted from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Look {
    steady_ms: u64,
    wall: i64,
    session_id: u32,
    /// The program in front then: the next step is its use.
    app: String,
    /// The most milliseconds the next step may count for that program: the
    /// time in hand then, less the part of a second already counted. None
    /// when there is no such limit (a day with no limit, an unlock, a
    /// program that does not count).
    cap_ms: Option<u64>,
}

/// The engine's own notes for one child, kept in the family's file beside
/// the device state and written with it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Notes {
    #[serde(default)]
    stretch: Option<Stretch>,
    #[serde(default)]
    look: Option<Look>,
}

impl Notes {
    /// True when `newer` only goes on from these notes: the same program's
    /// stretch, grown, and a later look in the same session at the same
    /// program.
    fn only_progress_to(&self, newer: &Notes) -> bool {
        let stretch = match (&self.stretch, &newer.stretch) {
            (Some(a), Some(b)) => a.app == b.app,
            (None, None) => true,
            _ => false,
        };
        let look = match (&self.look, &newer.look) {
            (Some(a), Some(b)) => a.session_id == b.session_id && a.app == b.app,
            _ => false,
        };
        stretch && look
    }
}

/// True when the hub says the device or the child is gone, so the engine
/// should let go of what it holds and pair again if the child is still
/// there.
fn gone(problem: &HubError) -> bool {
    matches!(
        problem,
        HubError::DeviceRemoved { .. }
            | HubError::UnknownDevice { .. }
            | HubError::NotMasterPc { .. }
            | HubError::UnknownChild { .. }
            | HubError::DeviceOfAnotherChild { .. }
    )
}

/// One child's device on the master PC.
pub(crate) struct ChildRun {
    pub(crate) child_id: String,
    pub(crate) device_id: String,
    pub(crate) state: DeviceState,
    notes: Notes,
    /// The state and notes as the family's file has them, when known.
    saved: Option<(DeviceState, Notes)>,
    /// The child's book revision after this run's last exchange.
    revision: Option<u64>,
    last_exchange_ms: Option<u64>,
    last_ask_ms: Option<u64>,
    /// The home day the last ask was for.
    last_ask_day: Option<i64>,
    /// When the state and notes were last kept, by the steady count.
    saved_at_ms: Option<u64>,
    quiet_until_ms: Option<u64>,
    /// A way out changed the state and its exchange has not gone through.
    pub(crate) exchange_soon: bool,
    /// The hub said the device or the child is gone.
    pub(crate) gone: bool,
    /// The decision behind the last orders, and when it was made.
    pub(crate) decision: Option<Decision>,
    pub(crate) decided_at: i64,
}

impl ChildRun {
    /// A run from what the family's file keeps. Notes that cannot be read
    /// are started afresh, and the problem is returned to be logged.
    pub(crate) fn from_record(record: MasterPcRecord) -> (ChildRun, Option<EngineError>) {
        let (notes, problem) = if record.notes.trim().is_empty() {
            (Notes::default(), None)
        } else {
            match serde_json::from_str::<Notes>(&record.notes) {
                Ok(notes) => (notes, None),
                Err(_) => {
                    let problem = HubError::Damaged { detail: "the agent's notes for a child could not be read".into() };
                    (Notes::default(), Some(EngineError::hub(HubStep::LoadChild, problem)))
                }
            }
        };
        let saved = problem.is_none().then(|| (record.state.clone(), notes.clone()));
        let run = ChildRun {
            child_id: record.child_id,
            device_id: record.device_id,
            state: record.state,
            notes,
            saved,
            revision: None,
            last_exchange_ms: None,
            last_ask_ms: None,
            last_ask_day: None,
            saved_at_ms: None,
            quiet_until_ms: None,
            exchange_soon: false,
            gone: false,
            decision: None,
            decided_at: i64::MIN,
        };
        (run, problem)
    }

    /// True when time with `app` counts against the budget, by the rules
    /// the device holds.
    pub(crate) fn counts(&self, app: &str) -> bool {
        !self.state.rules().is_always_allowed(app)
    }

    /// Counts the step from the last look to `at`. `active` is the child's
    /// session in use now, if any. Returns the milliseconds counted.
    pub(crate) fn take_step(&mut self, at: Moment, active: Option<&Session>, idle_limit_ms: u64) -> u64 {
        // The look is used once: any save from here on has none, so a
        // restart in the middle of this call can lose this step but never
        // count it twice.
        let Some(look) = self.notes.look.take() else { return 0 };
        let Some(step) = at.steady_ms.checked_sub(look.steady_ms) else {
            self.flush();
            return 0;
        };
        if step > MAX_COUNTED_STEP_MS {
            self.flush();
            return 0;
        }
        // The wall clock jumped within the step: what is open is recorded
        // where it was, and the step, measured by the steady count, starts
        // anew at the new time.
        let wall_ms = at.wall.saturating_sub(look.wall).saturating_mul(1_000);
        let off = wall_ms.saturating_sub(i64::try_from(step).unwrap_or(i64::MAX));
        if off.unsigned_abs() > CLOCK_JUMP_MS {
            self.flush();
        }
        let Some(session) = active.filter(|s| s.session_id == look.session_id) else {
            self.flush();
            return 0;
        };
        let idle_ms = u64::from(session.idle_secs).saturating_mul(1_000);
        let mut ms = if session.full_screen || idle_ms <= idle_limit_ms {
            step
        } else {
            // The count stopped when the idle minutes ran out after the last
            // input; the minutes before that count.
            let paused_at = at.steady_ms.saturating_sub(idle_ms).saturating_add(idle_limit_ms);
            paused_at.saturating_sub(look.steady_ms).min(step)
        };
        if let Some(cap) = look.cap_ms {
            ms = ms.min(cap);
        }
        if ms > 0 {
            match &mut self.notes.stretch {
                Some(open) if open.app == look.app => {
                    open.ms = open.ms.saturating_add(ms);
                    open.wall_end = at.wall;
                }
                _ => {
                    self.flush();
                    self.notes.stretch = Some(Stretch { app: look.app.clone(), ms, wall_end: at.wall });
                }
            }
        }
        if ms < step {
            self.flush();
        }
        ms
    }

    /// Records the whole seconds of the open stretch in the device state, as
    /// one stretch of use ending where the stretch was last counted.
    pub(crate) fn flush(&mut self) {
        if let Some(open) = &mut self.notes.stretch {
            let secs = open.ms / 1_000;
            if secs > 0 {
                let start = open.wall_end.saturating_sub(i64::try_from(secs).unwrap_or(i64::MAX));
                self.state.record_use(&open.app, start, open.wall_end);
                open.ms %= 1_000;
            }
        }
    }

    /// A copy of the device state with the open stretch recorded.
    pub(crate) fn probe_state(&self) -> DeviceState {
        let mut probe = self.state.clone();
        if let Some(open) = &self.notes.stretch {
            let secs = open.ms / 1_000;
            if secs > 0 {
                let start = open.wall_end.saturating_sub(i64::try_from(secs).unwrap_or(i64::MAX));
                probe.record_use(&open.app, start, open.wall_end);
            }
        }
        probe
    }

    /// The decision as it stands, without changing anything kept.
    pub(crate) fn probe(&self, now: i64, app: Option<&str>) -> Decision {
        self.probe_state().decide(now, app)
    }

    /// The decision behind the orders. When it gives a warning, or
    /// otherwise changes the warnings already given, the stretch is recorded
    /// and the device state decides itself, so that it keeps them and each
    /// warning comes once.
    pub(crate) fn decide(&mut self, now: i64, app: Option<&str>) -> Decision {
        let probe = self.probe(now, app);
        if probe.warned != self.state.holding_at(now).warned {
            self.flush();
            return self.state.decide(now, app);
        }
        probe
    }

    /// Ends this call's look: the child's time is being counted now in
    /// `session_id`, with `app` in front, or, with `None`, it is not. What
    /// is open for another program, or when counting stops, is recorded.
    pub(crate) fn set_look(&mut self, at: Moment, counting: Option<(u32, &str)>, decision: &Decision) {
        let look = counting.map(|(session_id, app)| {
            let part = self.notes.stretch.as_ref().filter(|s| s.app == app).map_or(0, |s| s.ms % 1_000);
            let cap_ms = match (decision.seconds_in_hand, decision.unlocked_until) {
                (Some(secs), None) if self.counts(app) => Some(secs.saturating_mul(1_000).saturating_sub(part)),
                _ => None,
            };
            Look { steady_ms: at.steady_ms, wall: at.wall, session_id, app: app.to_string(), cap_ms }
        });
        let goes_on = match (&look, &self.notes.stretch) {
            (Some(look), Some(open)) => look.app == open.app,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if !goes_on {
            self.flush();
        }
        self.notes.look = look;
    }

    /// True when the device has use, added time or a request the master
    /// has not confirmed, or use not yet recorded.
    pub(crate) fn has_unreported(&self) -> bool {
        self.exchange_soon
            || !self.state.unconfirmed_slices().is_empty()
            || !self.state.unsent_added().is_empty()
            || self.notes.stretch.as_ref().is_some_and(|s| s.ms >= 1_000)
    }

    /// True while writes for this child wait after a failure.
    pub(crate) fn quiet(&self, at: Moment) -> bool {
        self.quiet_until_ms.is_some_and(|until| at.steady_ms < until)
    }

    /// True when an exchange with the child's book is due: the first call
    /// of this run; the book wants to hear from the device; the book changed
    /// since this run's last exchange (a guardian acted, another device
    /// reported); the device asks for time (`asking`) for home day `day`, at
    /// once for a new day and otherwise at most once a minute; use not yet
    /// reported, a minute after the last exchange; or a way out that has not
    /// reached the book.
    pub(crate) fn exchange_due(&self, at: Moment, wanted: bool, revision: Option<u64>, asking: bool, day: i64) -> bool {
        let since = |t: Option<u64>| t.map_or(u64::MAX, |t| at.steady_ms.checked_sub(t).unwrap_or(u64::MAX));
        let first = self.last_exchange_ms.is_none();
        let changed = revision.is_some_and(|r| self.revision != Some(r));
        let ask = asking && (self.last_ask_day != Some(day) || since(self.last_ask_ms) >= ASK_AGAIN_MS);
        let regular = self.has_unreported() && since(self.last_exchange_ms) >= REPORT_EVERY_SECS * 1_000;
        first || wanted || changed || ask || regular || self.exchange_soon
    }

    /// The agent's notes as the family's file keeps them.
    fn notes_json(&self) -> String {
        serde_json::to_string(&self.notes).unwrap_or_default()
    }

    /// One exchange with the child's book: what is open is recorded first,
    /// so the report carries all the use; the book, the device state and
    /// the notes are written together.
    pub(crate) fn exchange(&mut self, hub: &mut Hub, want_more: bool, at: Moment) -> Result<(), EngineError> {
        self.flush();
        let notes = self.notes_json();
        match hub.exchange_on_master_pc(&mut self.state, want_more, &notes, at.wall) {
            Ok(done) => {
                self.revision = Some(done.book_revision);
                self.last_exchange_ms = Some(at.steady_ms);
                if want_more {
                    self.last_ask_ms = Some(at.steady_ms);
                    self.last_ask_day = Some(done.grant.day);
                }
                self.exchange_soon = false;
                self.quiet_until_ms = None;
                self.saved = Some((self.state.clone(), self.notes.clone()));
                self.saved_at_ms = Some(at.steady_ms);
                Ok(())
            }
            Err(problem) => Err(self.failed(HubStep::Exchange, problem, at)),
        }
    }

    /// Keeps the device state and the notes when they changed since they
    /// were last kept: at once, unless the only change is counting going on
    /// as before, which is kept every few seconds.
    pub(crate) fn save_if_changed(&mut self, hub: &mut Hub, at: Moment) -> Result<(), EngineError> {
        if let Some((state, notes)) = &self.saved
            && *state == self.state
        {
            let recent = self.saved_at_ms.is_some_and(|t| at.steady_ms.checked_sub(t).is_some_and(|d| d < SAVE_PROGRESS_EVERY_MS));
            if *notes == self.notes || (recent && notes.only_progress_to(&self.notes)) {
                return Ok(());
            }
        }
        match hub.save_master_pc(&self.state, &self.notes_json(), at.wall) {
            Ok(()) => {
                self.saved = Some((self.state.clone(), self.notes.clone()));
                self.saved_at_ms = Some(at.steady_ms);
                self.quiet_until_ms = None;
                Ok(())
            }
            Err(problem) => Err(self.failed(HubStep::Save, problem, at)),
        }
    }

    /// Keeps the device state and the notes now, if they changed at all.
    pub(crate) fn save_now(&mut self, hub: &mut Hub) -> Result<(), EngineError> {
        if self.saved.as_ref().is_some_and(|(state, notes)| *state == self.state && *notes == self.notes) {
            return Ok(());
        }
        hub.save_master_pc(&self.state, &self.notes_json(), self.decided_at)
            .map_err(|problem| EngineError::hub(HubStep::Save, problem))?;
        self.saved = Some((self.state.clone(), self.notes.clone()));
        Ok(())
    }

    fn failed(&mut self, step: HubStep, problem: HubError, at: Moment) -> EngineError {
        self.gone |= gone(&problem);
        self.quiet_until_ms = Some(at.steady_ms.saturating_add(QUIET_AFTER_FAILURE_MS));
        EngineError::hub(step, problem)
    }

    /// Whole seconds of use not yet recorded in the device state.
    pub(crate) fn open_secs(&self) -> u64 {
        self.notes.stretch.as_ref().map_or(0, |s| s.ms / 1_000)
    }

    /// The slices the master has not confirmed, with the open stretch as
    /// one more.
    pub(crate) fn unreported(&self) -> Vec<Slice> {
        self.probe_state().unconfirmed_slices()
    }
}
