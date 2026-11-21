//! The [`PhoneEngine`]: called about once a second with the facts, it
//! returns the orders; it writes the reports the host carries to the master
//! and takes the grants that come back; called from the lock, it takes the
//! ways out; and it hands the host the bytes to keep.

use limo_core::{
    AddedTime, Decision, DeviceState, Grant, GrantOutcome, LockReason, PinAction, Report, RequestView, RuleSet, Slice,
    TimeRequest, TryOutcome, WarningCause,
};
use serde::{Deserialize, Serialize};

use crate::counts;
use crate::error::{LoadProblem, PhoneError};
use crate::facts::{CallState, GuardedPage, PhoneFacts};
use crate::kept::{Change, Kept, Moment};
use crate::orders::{
    Cover, GrantTaken, GuardScreen, LockScreen, LockWhy, PhoneNote, PhoneOrders, RequestLine, TimeLeft, TryAnswer,
    WarningShown, Way, WayRow,
};
use crate::today::{self, RulesWords, TodayView};
use crate::words::{self, PhoneClock};
use crate::{ANSWER_WAIT_MS, GUARD_OPEN_SECS, REPORT_EVERY_SECS, REPORT_IDLE_SECS, SAVE_PROGRESS_EVERY_MS};

/// A moment by the elapsed count, on one start of the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    boot: u32,
    ms: u64,
}

/// Milliseconds from `from` to `to`; as long as can be when `from` is not
/// known, is on another start of the phone, or is later.
fn since(from: Option<Stamp>, to: Stamp) -> u64 {
    match from {
        Some(from) if from.boot == to.boot => to.ms.checked_sub(from.ms).unwrap_or(u64::MAX),
        _ => u64::MAX,
    }
}

/// What the last call saw.
#[derive(Debug, Clone, Copy)]
struct Seen {
    stamp: Stamp,
    /// LiMo's clock.
    now: i64,
    clock: PhoneClock,
    /// The child looks at the phone and the core says to ask for more: a
    /// report now asks for it.
    asking: bool,
}

/// How the talk with the master stands. Kept in memory only: after a
/// restart the engine reports at the first call in reach.
#[derive(Debug, Clone, Default)]
struct Talk {
    last_report: Option<Stamp>,
    /// A report went and no grant or failure has come back yet.
    awaiting: bool,
    /// The last report got no answer.
    failed: bool,
    last_grant: Option<Stamp>,
    last_ask: Option<Stamp>,
    /// The home day the last ask was for.
    last_ask_day: Option<i64>,
    /// A way out changed the state and has not reached the master.
    way_out: bool,
    /// A note of a kind the master takes was made since the last grant.
    note_made: bool,
    /// A grant asked for a report at once.
    ack_due: bool,
}

impl Talk {
    /// True when a report is due, the master being in reach: the first since
    /// the engine started; the master asked for one; the phone asks for time
    /// (at once for a new day, otherwise at most once a minute while it still
    /// needs time); use not yet reported and a minute since the last
    /// exchange; five minutes since the last exchange in any case; a way out
    /// or a note for the master that has not reached it; a grant that asked
    /// for a report at once. Not while a report waits for its answer, nor
    /// within ten seconds of one that got none.
    fn due(&self, at: Stamp, prodded: bool, asking: bool, day: i64, unreported: bool) -> bool {
        if (self.awaiting || self.failed) && since(self.last_report, at) < ANSWER_WAIT_MS {
            return false;
        }
        let first = self.last_grant.is_none();
        let ask = asking
            && (self.last_ask_day != Some(day) || since(self.last_ask, at) >= limo_core::RETRY_SECS as u64 * 1_000);
        let quiet = since(self.last_grant, at);
        let regular = unreported && quiet >= REPORT_EVERY_SECS * 1_000;
        let idle = quiet >= REPORT_IDLE_SECS * 1_000;
        first || prodded || ask || regular || idle || self.way_out || self.note_made || self.ack_due
    }
}

/// The phone as the engine holds it now: for the child's own screens
/// (`today` and `rules_words`, with every word in them), and for tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhoneView {
    /// The child.
    pub child_id: String,
    /// This phone, as the master knows it.
    pub device_id: String,
    /// The rules the phone holds.
    pub rules: RuleSet,
    /// The core's decision behind the last orders.
    pub decision: Option<Decision>,
    /// The home day of that decision.
    pub day: i64,
    /// Counted use on this phone that day, reported or not, in seconds.
    pub counted_secs: u64,
    /// The part of it beyond everything the phone held that day: use while a
    /// guardian's unlock kept it open with nothing in hand. 0 otherwise.
    pub over_secs: u64,
    /// Whole seconds of use counted but not yet recorded in the device
    /// state.
    pub open_secs: u64,
    /// Use the master has not confirmed yet, the open stretch included.
    pub unreported: Vec<Slice>,
    /// Time added on the phone that the master has not confirmed yet.
    pub unsent_added: Vec<AddedTime>,
    /// Requests for more time made for today on any of the child's devices,
    /// as far as this phone knows, oldest first.
    pub requests_today: Vec<RequestView>,
    /// Notes for the guardian the master has not taken yet, oldest first:
    /// those of a kind the master takes until a grant confirms them, and the
    /// newest of the kinds that stay on the phone.
    pub notes: Vec<PhoneNote>,
    /// How far the phone's wall clock is ahead of LiMo's clock, in seconds:
    /// the jumps LiMo did not follow.
    pub clock_correction_secs: i64,
    /// The child's Today screen, at the last call.
    pub today: TodayView,
    /// The rules in words, for How it works.
    pub rules_words: RulesWords,
    /// Whether setup is finished on this phone.
    pub setup: SetupView,
}

/// Whether a guardian has finished setup on the phone. Until then the engine
/// counts nothing and covers nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupView {
    /// True once setup is finished (or the state was kept by a LiMo that
    /// did not know of setup, whose phone was in use already).
    pub finished: bool,
    /// When it was finished, by LiMo's clock; None while it is not, and for
    /// a state kept before setup was kept.
    pub finished_at: Option<i64>,
    /// "Setup is not finished on this phone." while it is not; None once it
    /// is.
    pub words: Option<String>,
}

/// The engine of LiMo on a child's phone. It holds the core's device state
/// for the phone and what it has counted, and does nothing on its own: it
/// has no clock, no files and no network.
pub struct PhoneEngine {
    /// None when the kept state could not be loaded.
    kept: Option<Kept>,
    problem: Option<LoadProblem>,
    /// What the host last kept, when known.
    saved: Option<Kept>,
    saved_at: Option<Stamp>,
    seen: Option<Seen>,
    talk: Talk,
    decision: Option<Decision>,
}

impl std::fmt::Debug for PhoneEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhoneEngine")
            .field("device_id", &self.kept.as_ref().map(|k| k.state.device_id()))
            .field("problem", &self.problem)
            .finish_non_exhaustive()
    }
}

impl PhoneEngine {
    fn with(kept: Option<Kept>, problem: Option<LoadProblem>, saved: Option<Kept>) -> PhoneEngine {
        PhoneEngine { kept, problem, saved, saved_at: None, seen: None, talk: Talk::default(), decision: None }
    }

    /// A new phone, from what the master made at pairing: the welcome grant
    /// and the phone's 32 byte code secret, at `now` (the wall clock). Keep
    /// it at once ([`PhoneEngine::save`]). Setup is not finished on it yet:
    /// it counts and covers nothing until [`PhoneEngine::finish_setup`].
    pub fn paired(welcome: &Grant, code_secret: Vec<u8>, now: i64) -> Result<PhoneEngine, PhoneError> {
        let mut state = DeviceState::from_pairing(welcome, now);
        state.set_code_secret(code_secret).map_err(|problem| PhoneError::Refused { problem })?;
        Ok(PhoneEngine::with(Some(Kept::new(state)), None, None))
    }

    /// The phone from the bytes the host kept. It never fails: a state that
    /// cannot be loaded (empty, not LiMo's, written by a newer LiMo, or
    /// damaged) gives an engine that **fails safe**: it covers the phone with
    /// a lock that says LiMo cannot check the time left, with no way out on
    /// the phone (the PIN record and the code secret were in the state),
    /// while calls, the dialler, the emergency dialler and LiMo itself stay
    /// open. It reports nothing, takes no grant, and never asks to be kept,
    /// so the bytes stay as they are for a guardian, and a state from a
    /// newer LiMo is still there when that LiMo is back. Pairing the phone
    /// again replaces it. [`PhoneEngine::load_problem`] says what was wrong.
    pub fn load(bytes: &[u8]) -> PhoneEngine {
        match Kept::decode(bytes) {
            Ok(kept) => PhoneEngine::with(Some(kept.clone()), None, Some(kept)),
            Err(problem) => PhoneEngine::with(None, Some(problem), None),
        }
    }

    /// Why the kept state could not be loaded, if it could not.
    pub fn load_problem(&self) -> Option<LoadProblem> {
        self.problem
    }

    /// One call: takes the facts, counts the time since the last call, and
    /// returns the orders. It never fails.
    pub fn tick(&mut self, facts: &PhoneFacts) -> PhoneOrders {
        let clock = PhoneClock { utc_offset_secs: facts.utc_offset_secs, hours_24: facts.hours_24 };
        let Some(kept) = self.kept.as_mut() else { return cannot_check(facts, clock) };
        // Until a guardian finishes setup on this phone, nothing is counted,
        // covered or warned of, the master is not asked for time, and no
        // permission switched off is noted: the guardian is holding the
        // phone, on Android's own pages, and that is not the child's time.
        let setting_up = !kept.setup_finished();
        let mut notes = Vec::new();
        let now = kept.watch_clock(facts, &mut notes);
        kept.watch_permissions(facts.permissions, now, &mut notes);
        let at = Moment { wall: now, elapsed_ms: facts.elapsed_ms, boot: facts.boot_count };
        let stamp = Stamp { boot: facts.boot_count, ms: facts.elapsed_ms };
        if !facts.in_reach {
            // Out of reach, what the master last said is left is not trusted:
            // warnings follow the time in hand.
            kept.state.lost_contact();
        }
        kept.take_step(at, &mut notes);

        // The decision behind the orders, for the app in front, and whether
        // a report is due. A warning floats over the app in front, so it is
        // kept only where a cover may be drawn (never over a call's screen,
        // the diallers or LiMo), and not in a call that sends a report
        // asking for more time: the call right after the grant decides with
        // what the master gave, as the PC's engine exchanges before it
        // decides.
        let screen = counts::screen_in_use(facts);
        let front_app = facts.front.rules_app();
        let probe = kept.probe(now, front_app.as_deref());
        let asking = probe.ask_master && screen && !setting_up;
        let unreported = kept.has_unreported();
        let send_report = facts.in_reach && self.talk.due(stamp, facts.prodded, asking, probe.day, unreported);
        let may_cover = counts::may_cover(facts) && !setting_up;
        let decision = if may_cover && !(send_report && asking) {
            kept.keep_warnings(now, front_app.as_deref(), probe)
        } else {
            Decision { warning: None, ..probe }
        };
        let rules = kept.state.rules();
        let why = decision.device_lock.filter(|_| !setting_up).map(|lock| LockWhy::from_core(lock.reason, facts.in_reach));
        let app_blocked = !setting_up && decision.lock.is_some_and(|lock| lock.reason == LockReason::AppBlocked);
        // A page that could switch LiMo off or remove it is guarded, unless
        // a right PIN keeps such pages open (until the phone is put down, the
        // phone starts again, or the time is up), or setup is not finished.
        let guard_open = kept.guard_open_now(facts.boot_count, facts.elapsed_ms, screen);
        let guarding =
            facts.guarded.filter(|page| !setting_up && !guard_open && page.needs_guard(facts.permissions));

        // What to draw. The lock covers whatever is in front but the apps it
        // names; a blocked app is covered on its own; a guarded page is
        // covered by the guard; nothing is ever drawn over a ringing call,
        // the diallers, LiMo or Android's own lock screen. During a call in
        // progress any other app is covered as with no call, and the cover
        // offers a way back to the call.
        let call_on = facts.call == CallState::Ongoing;
        let back_to_call = call_on.then(|| words::BACK_TO_THE_CALL.to_string());
        let cover = match (may_cover, why, decision.device_lock) {
            (false, _, _) => Cover::Nothing,
            (true, Some(why), Some(lock)) if !decision.allowed => {
                let latest = kept.state.requests_today(now).pop();
                let mut screen = lock_screen(
                    why,
                    lock.until,
                    Some(&decision),
                    latest,
                    rules.always_allowed_apps.clone(),
                    now,
                    clock,
                );
                screen.call_on = call_on;
                screen.back_to_call = back_to_call;
                Cover::Lock(Box::new(screen))
            }
            (true, _, _) if app_blocked => Cover::Block {
                package: facts.front.package().unwrap_or_default().to_string(),
                title: words::block_title(facts.front.label()),
                message: LockWhy::AppBlocked.message().to_string(),
                go_home: true,
                call_on,
                back_to_call,
            },
            (true, _, _) => match guarding {
                Some(page) => {
                    let mut screen = guard_screen(page, &decision, now, clock);
                    screen.call_on = call_on;
                    screen.back_to_call = back_to_call;
                    Cover::Guard(Box::new(screen))
                }
                None => Cover::Nothing,
            },
        };

        // Whether time is counted now, and the look the next step counts
        // from: an app in use that is not covered.
        let using =
            counts::app_in_use(facts).filter(|_| !setting_up && decision.allowed && cover == Cover::Nothing);
        kept.set_look(at, using.as_deref(), &decision);
        let counting = using.is_some() && decision.counts;

        match &cover {
            Cover::Block { package, .. } => kept.note_blocked(package, decision.day, now, &mut notes),
            Cover::Guard(screen) => kept.note_guarded(screen.page, decision.day, now, &mut notes),
            Cover::Nothing | Cover::Lock(_) => {}
        }
        self.talk.note_made |= kept.keep_notes(&notes);

        // A warning, only where it can be read.
        let warning = decision.warning.map(|w| {
            let away = w.cause == WarningCause::TimeRunsOut
                && !facts.in_reach
                && kept.coming_lock(now, w.lock_at) == Some(LockReason::NeedsMaster);
            let said = words::warning_words(w.cause, w.minutes, w.lock_at, now, away, clock);
            WarningShown {
                cause: w.cause,
                minutes: w.minutes,
                lock_at: w.lock_at,
                away,
                message: said.message,
                hint: said.hint,
                when: said.when,
            }
        });

        let time_left = TimeLeft {
            seconds_left: decision.seconds_left,
            counting,
            unlocked_until: decision.unlocked_until,
            away: !facts.in_reach,
            locked: why.is_some(),
            words: match why {
                _ if setting_up => words::SETUP_NOT_FINISHED.to_string(),
                Some(why) => why.title().to_string(),
                None => words::time_left_words(decision.seconds_left, counting, decision.unlocked_until, now, clock),
            },
        };
        let next_check = if screen { decision.next_check_at } else { decision.next_check_if_idle_at };
        let next_check_in_secs = u64::try_from(next_check.saturating_sub(now)).unwrap_or(0).max(1);
        self.seen = Some(Seen { stamp, now, clock, asking });
        self.decision = Some(decision);
        PhoneOrders {
            cover,
            time_left,
            warning,
            send_report,
            save_state: self.should_save(),
            notes,
            next_check_in_secs,
        }
    }

    /// The moment of the last call, by the elapsed count.
    fn stamp(&self) -> Option<Stamp> {
        self.seen.map(|s| s.stamp)
    }

    /// The phone's clock as the last call saw it.
    fn clock(&self) -> PhoneClock {
        self.seen.map(|s| s.clock).unwrap_or_default()
    }

    /// The report to send to the master now, at `now` (the wall clock): all
    /// the use the master has not confirmed, the time added and the
    /// requests made on the phone, every note for the guardian of a kind the
    /// master takes that it has not confirmed, and, while the child is at
    /// the phone and the core says so, a request for more time. Keep the
    /// state ([`PhoneEngine::save`]) before sending it. None when the kept
    /// state could not be loaded.
    pub fn outgoing_report(&mut self, now: i64) -> Option<Report> {
        let kept = self.kept.as_mut()?;
        let now = kept.limo_time(now);
        kept.flush();
        let want_more = self.seen.is_some_and(|s| s.asking);
        let mut report = kept.state.report(now, want_more);
        report.notes = kept.outgoing_notes();
        let stamp = self.seen.map(|s| s.stamp);
        self.talk.last_report = stamp;
        self.talk.awaiting = true;
        self.talk.failed = false;
        self.talk.ack_due = false;
        if want_more {
            self.talk.last_ask = stamp;
            self.talk.last_ask_day = Some(report.day);
        }
        Some(report)
    }

    /// Takes the master's answer to a report, at `now` (the wall clock): the
    /// notes it confirms are dropped, from any grant for this phone. Keep
    /// the state ([`PhoneEngine::save`]) before acting on it, and call
    /// [`PhoneEngine::tick`] again at once so that the orders follow it.
    /// When it says `report_again`, send another report now.
    pub fn take_grant(&mut self, grant: &Grant, now: i64) -> GrantTaken {
        self.talk.awaiting = false;
        let Some(kept) = self.kept.as_mut() else {
            return GrantTaken { outcome: GrantOutcome::NotForThisDevice, report_again: false };
        };
        let now = kept.limo_time(now);
        kept.flush();
        let outcome = kept.state.take_grant(grant, now);
        if outcome != GrantOutcome::NotForThisDevice {
            kept.confirm_notes(&grant.confirmed_notes);
            self.talk.failed = false;
            self.talk.last_grant = self.seen.map(|s| s.stamp);
            self.talk.way_out = false;
            self.talk.note_made = false;
        }
        let report_again = outcome == GrantOutcome::Taken && grant.acknowledge_now;
        self.talk.ack_due |= report_again;
        GrantTaken { outcome, report_again }
    }

    /// Call when a report got no answer in time
    /// ([`crate::ANSWER_WAIT_MS`]), or the master could not be reached. The
    /// phone then stops trusting what the master last said is left, so that
    /// its warnings follow the time it holds. Calling it while out of reach
    /// is not needed: the facts say so.
    pub fn lost_contact(&mut self) {
        self.talk.awaiting = false;
        self.talk.failed = true;
        if let Some(kept) = self.kept.as_mut() {
            kept.state.lost_contact();
        }
    }

    /// The child asks for `minutes` more today, with a short reason or none,
    /// at `now` (the wall clock). The request goes with the next report, and
    /// with every one after until the master has it. Refused, in the core's
    /// words, when it cannot be made now: one is already waiting, today has
    /// no limit, or too many were made today. Keep the state at once.
    pub fn ask_for_time(&mut self, minutes: u32, reason: &str, now: i64) -> Result<TimeRequest, PhoneError> {
        let kept = self.kept.as_mut().ok_or(PhoneError::CannotCheck)?;
        let now = kept.limo_time(now);
        kept.flush();
        let request = kept.state.ask_for_time(minutes, reason, now).map_err(|problem| PhoneError::Refused { problem })?;
        self.talk.way_out = true;
        Ok(request)
    }

    /// A one time code typed on the lock, at `now` (the wall clock). A right
    /// code adds its minutes on this phone for today, once; a wrong one
    /// counts as a wrong guess, and after five the next try must wait. Keep
    /// the state at once, right or wrong, before showing the answer.
    pub fn try_code(&mut self, typed: &str, now: i64) -> Result<TryAnswer, PhoneError> {
        let kept = self.kept.as_mut().ok_or(PhoneError::CannotCheck)?;
        let now = kept.limo_time(now);
        kept.flush();
        let outcome = kept.state.try_code(typed, now);
        Ok(self.answer(Way::Code, outcome, now))
    }

    /// A guardian types the PIN on the lock, at `now` (the wall clock), to
    /// add minutes, to unlock until a time, or to confirm a release (the
    /// engine only says the PIN was right; releasing the phone is the
    /// host's). Wrong PINs count with wrong codes. An action that could not
    /// be done is refused before the PIN is looked at. Keep the state at
    /// once, right or wrong, before showing the answer.
    pub fn try_pin(&mut self, pin: &str, action: PinAction, now: i64) -> Result<TryAnswer, PhoneError> {
        let kept = self.kept.as_mut().ok_or(PhoneError::CannotCheck)?;
        let now = kept.limo_time(now);
        kept.flush();
        let outcome = kept.state.try_pin(pin, action, now).map_err(|problem| PhoneError::Refused { problem })?;
        Ok(self.answer(Way::Pin, outcome, now))
    }

    /// A guardian types the PIN on the guard, at `now` (the wall clock), to
    /// open the pages that could switch LiMo off or remove it
    /// ([`crate::GuardedPage`]). A right PIN keeps them open for
    /// [`crate::GUARD_OPEN_SECS`], counted by the elapsed count from the last
    /// call, or until the screen goes off, Android's lock screen shows or the
    /// phone starts again, whichever comes first; the guardian is told by a
    /// note ([`PhoneNote::GuardOpened`]). It changes nothing else: no time is
    /// added and nothing is unlocked. Wrong PINs count with wrong codes and
    /// PINs on the lock, with the same wait after five. Keep the state at
    /// once, right or wrong, before showing the answer, then tick.
    ///
    /// The core has no action that only checks the PIN, so the check is its
    /// `PinAction::ConfirmRelease`, which does nothing but say whether the
    /// PIN was right and clear the wrong tries when it was; the answer's
    /// outcome is then `ReleaseConfirmed`.
    pub fn try_guard_pin(&mut self, pin: &str, now: i64) -> Result<TryAnswer, PhoneError> {
        let from = self.seen.map(|s| (s.stamp.boot, s.stamp.ms));
        let clock = self.clock();
        let kept = self.kept.as_mut().ok_or(PhoneError::CannotCheck)?;
        let now = kept.limo_time(now);
        kept.flush();
        let outcome =
            kept.state.try_pin(pin, PinAction::ConfirmRelease, now).map_err(|problem| PhoneError::Refused { problem })?;
        if outcome != TryOutcome::ReleaseConfirmed {
            return Ok(self.answer(Way::Pin, outcome, now));
        }
        let until = now.saturating_add(i64::try_from(GUARD_OPEN_SECS).unwrap_or(i64::MAX));
        kept.open_guard(from, until);
        self.talk.note_made |= kept.keep_notes(&[PhoneNote::GuardOpened { until, at: now }]);
        self.talk.way_out = true;
        Ok(TryAnswer { message: words::guard_open_words(now, until, clock), opens_now: None, outcome })
    }

    /// Until when a right guardian PIN keeps the guarded pages open, by
    /// LiMo's clock; None while they are guarded. As at the last call.
    pub fn guard_open_until(&self) -> Option<i64> {
        self.kept.as_ref().and_then(|k| k.guard_until())
    }

    /// A guardian finished setup on this phone, at `now` (the wall clock):
    /// the moment they leave the setup's last screen, whether every step is
    /// on or one was passed. Until then the engine counts nothing and covers
    /// nothing; from the next call on it does both, as it does for any
    /// phone. Once finished it stays finished, in the kept state: only a new
    /// pairing (a new state) starts setup again. The guardian is told by a
    /// note ([`PhoneNote::SetupFinished`]), once.
    ///
    /// True when this call finished it, false when it was finished already,
    /// which changes nothing. Keep the state at once. Refused with
    /// [`PhoneError::CannotCheck`] when the kept state could not be loaded.
    pub fn finish_setup(&mut self, now: i64) -> Result<bool, PhoneError> {
        let kept = self.kept.as_mut().ok_or(PhoneError::CannotCheck)?;
        if kept.setup_finished() {
            return Ok(false);
        }
        let now = kept.limo_time(now);
        let finished = kept.finish_setup(now);
        // The note goes to the master at the next call in reach, so a PC
        // waiting for setup to finish hears of it within seconds.
        self.talk.note_made |= finished;
        Ok(finished)
    }

    /// The answer to a code or a PIN, in words, and the master to hear of it.
    fn answer(&mut self, way: Way, outcome: TryOutcome, now: i64) -> TryAnswer {
        self.talk.way_out = true;
        let clock = self.clock();
        let right = matches!(outcome, TryOutcome::Added { .. } | TryOutcome::Unlocked { .. });
        let open = self.kept.as_ref().is_some_and(|k| k.probe(now, None).device_lock.is_none());
        TryAnswer {
            message: words::try_words(way, &outcome, now, clock),
            opens_now: (right && open).then(|| words::OPENS_NOW.to_string()),
            outcome,
        }
    }

    /// True when the host should keep the state now: something changed that
    /// must survive a restart (a slice, a grant, a report written, a way
    /// out, a note, a warning given), or counting has gone on for five
    /// seconds since the state was last kept. False when the kept state
    /// could not be loaded: those bytes are left as they are.
    pub fn should_save(&self) -> bool {
        let Some(kept) = &self.kept else { return false };
        let Some(saved) = &self.saved else { return true };
        match kept.change_since(saved) {
            Change::Nothing => false,
            Change::Other => true,
            Change::Progress => {
                self.stamp().is_none_or(|at| since(self.saved_at, at) >= SAVE_PROGRESS_EVERY_MS)
            }
        }
    }

    /// The bytes to keep: write them to the state file, in place of what was
    /// there, before acting on anything else. Empty when the kept state
    /// could not be loaded: then write nothing.
    pub fn save(&mut self) -> Vec<u8> {
        let Some(kept) = &self.kept else { return Vec::new() };
        let bytes = kept.encode();
        self.saved = Some(kept.clone());
        self.saved_at = self.stamp();
        bytes
    }

    /// Call when the bytes [`PhoneEngine::save`] gave could not be written:
    /// the engine then asks to be kept again at the next call.
    pub fn save_failed(&mut self) {
        self.saved = None;
    }

    /// The phone as the engine holds it now, or None when the kept state
    /// could not be loaded. Today and the rules in words are as at the last
    /// call; before any call, as at the last moment kept.
    pub fn view(&self) -> Option<PhoneView> {
        let kept = self.kept.as_ref()?;
        let now = self.seen.map_or(i64::MIN, |s| s.now);
        let probe = kept.probe_state();
        let holding = probe.holding_at(now);
        let day = self.decision.as_ref().map_or(holding.day, |d| d.day);
        let (counted_secs, over_secs) =
            if holding.day == day { (holding.used_secs(), holding.over_secs()) } else { (0, 0) };
        let rules = kept.state.rules();
        let (at, clock) = match (self.seen, kept.last_seen()) {
            (Some(seen), _) => (seen.now, seen.clock),
            (None, Some((at, offset))) => (at, PhoneClock { utc_offset_secs: offset, hours_24: false }),
            (None, None) => (rules.calendar().day_start(holding.day), PhoneClock::default()),
        };
        let decision = match &self.decision {
            Some(decision) => decision.clone(),
            None => probe.clone().decide(at, None),
        };
        let others = probe.others_used(day);
        let today = today::today(
            &rules,
            &decision,
            today::Used { here_secs: counted_secs, others },
            kept.apps_on(day),
            kept.permissions(),
            at,
            clock,
        );
        Some(PhoneView {
            child_id: kept.state.child_id(),
            device_id: kept.state.device_id(),
            rules_words: today::rules_words(&rules, at, clock),
            rules,
            decision: self.decision.clone(),
            day,
            counted_secs,
            over_secs,
            open_secs: kept.open_secs(),
            unreported: kept.unreported(),
            unsent_added: kept.state.unsent_added(),
            requests_today: kept.state.requests_today(now),
            notes: kept.notes(),
            clock_correction_secs: kept.correction_secs(),
            today,
            setup: SetupView {
                finished: kept.setup_finished(),
                finished_at: kept.setup_finished_at(),
                words: (!kept.setup_finished()).then(|| words::SETUP_NOT_FINISHED.to_string()),
            },
        })
    }
}

/// True for a lock that time can open: a code adds time, and is offered
/// only for these.
fn opened_by_time(why: LockWhy) -> bool {
    matches!(
        why,
        LockWhy::NoTimeToday
            | LockWhy::TimeUsedUp
            | LockWhy::WaitingForTime
            | LockWhy::AwayTimeUsedUp
            | LockWhy::HeldByAnotherDevice
    )
}

/// The lock, with its words and its ways out.
fn lock_screen(
    why: LockWhy,
    until: Option<i64>,
    decision: Option<&Decision>,
    latest_request: Option<RequestView>,
    allowed_packages: Vec<String>,
    now: i64,
    clock: PhoneClock,
) -> LockScreen {
    let until_words = match until {
        Some(at) => Some(words::opens_again(now, at, clock)),
        None => why.until_without_a_time().map(str::to_string),
    };
    let can_ask_for_time = decision.is_some_and(|d| d.can_ask_for_time);
    let can_use_code = decision.is_some_and(|d| d.can_use_code) && opened_by_time(why);
    let can_use_pin = decision.is_some_and(|d| d.can_use_pin);
    let try_wait_until = decision.and_then(|d| d.try_wait_until).filter(|w| *w > now);
    let ways: Vec<WayRow> = [(Way::Ask, can_ask_for_time), (Way::Code, can_use_code), (Way::Pin, can_use_pin)]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(way, _)| {
            let (title, sub) = way.words();
            let off = way != Way::Ask && try_wait_until.is_some();
            let sub = match try_wait_until {
                Some(until) if off => words::wait_row(now, until, clock),
                _ => sub.to_string(),
            };
            WayRow { way, title: title.to_string(), sub, off }
        })
        .collect();
    let nothing_to_do = ways.is_empty().then(|| {
        let (bold, line) = why.nothing_to_do();
        (bold.to_string(), line.to_string())
    });
    let request_line = latest_request.as_ref().map(|r| {
        let (text, stamp) = words::request_words(r, clock);
        RequestLine { text, stamp }
    });
    LockScreen {
        why,
        stamp: why.stamp().to_string(),
        title: why.title().to_string(),
        message: why.message().to_string(),
        until,
        until_words,
        allowed_packages,
        can_ask_for_time,
        can_use_code,
        can_use_pin,
        try_wait_until,
        wait_words: try_wait_until.map(|until| words::wait_words(now, until, clock)),
        latest_request,
        request_line,
        ways,
        nothing_to_do,
        call_on: false,
        back_to_call: None,
    }
}

/// The guard, with its words and its one way out, the guardian PIN: offered
/// when the phone holds a PIN record, greyed with the lock's wait after
/// wrong tries.
fn guard_screen(page: GuardedPage, decision: &Decision, now: i64, clock: PhoneClock) -> GuardScreen {
    let can_use_pin = decision.can_use_pin;
    let try_wait_until = decision.try_wait_until.filter(|w| *w > now);
    let ways: Vec<WayRow> = can_use_pin
        .then(|| {
            let (title, _) = Way::Pin.words();
            let sub = match try_wait_until {
                Some(until) => words::wait_row(now, until, clock),
                None => words::guard_row(GUARD_OPEN_SECS),
            };
            WayRow { way: Way::Pin, title: title.to_string(), sub, off: try_wait_until.is_some() }
        })
        .into_iter()
        .collect();
    let nothing_to_do = ways.is_empty().then(|| {
        let (bold, line) = words::GUARD_NO_PIN;
        (bold.to_string(), line.to_string())
    });
    GuardScreen {
        page,
        stamp: words::GUARD_STAMP.to_string(),
        title: words::GUARD_TITLE.to_string(),
        message: words::GUARD_MESSAGE.to_string(),
        can_use_pin,
        try_wait_until,
        wait_words: try_wait_until.map(|until| words::wait_words(now, until, clock)),
        ways,
        nothing_to_do,
        call_on: false,
        back_to_call: None,
    }
}

/// The orders of an engine whose kept state could not be loaded: the phone
/// is locked, as LiMo cannot check the time left, and nothing else.
fn cannot_check(facts: &PhoneFacts, clock: PhoneClock) -> PhoneOrders {
    let why = LockWhy::CannotCheck;
    let cover = if counts::may_cover(facts) {
        let mut screen = lock_screen(why, None, None, None, Vec::new(), facts.wall_secs, clock);
        screen.call_on = facts.call == CallState::Ongoing;
        screen.back_to_call = screen.call_on.then(|| words::BACK_TO_THE_CALL.to_string());
        Cover::Lock(Box::new(screen))
    } else {
        Cover::Nothing
    };
    PhoneOrders {
        cover,
        time_left: TimeLeft {
            seconds_left: None,
            counting: false,
            unlocked_until: None,
            away: !facts.in_reach,
            locked: true,
            words: why.title().to_string(),
        },
        warning: None,
        send_report: false,
        save_state: false,
        notes: Vec::new(),
        next_check_in_secs: limo_core::RETRY_SECS as u64,
    }
}
