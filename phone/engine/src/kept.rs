//! What the phone keeps: the core's device state and the engine's own notes
//! (the stretch of use not yet recorded, the last look at which the child's
//! time was being counted, LiMo's clock, the notes for the guardian, each
//! app's use today, whether setup is finished), as one versioned blob the
//! host writes.
//!
//! **Each app's use today**, for the child's Today screen, is added up where
//! the device state records use, from the slices it makes, so the two always
//! agree and survive a restart together. Each slice carries the home day the
//! core put it on (the rules' calendar, with the day start, never moving
//! back): a slice of a later day empties the count and starts that day.
//!
//! **How time is counted.** Between two calls the engine measures the step
//! with the elapsed count, never with the wall clock. A step counts when the
//! child was using an app at the look before it (see
//! [`crate::app_in_use`]) and that app was not covered, on the same start of
//! the phone. It belongs to that app, and counts in full: the host calls at
//! once when anything changes, so a step ends where the use ended. It is
//! never more than the time the phone held at the look before. A step
//! longer than [`crate::MAX_COUNTED_STEP_MS`] counts nothing: LiMo was held
//! up or stopped, and what happened meanwhile is not known. A step across a
//! restart of the phone counts nothing.
//!
//! **Why a stretch.** Counted milliseconds gather in a stretch for the app
//! in use and go into the device state as whole seconds, as one slice, at a
//! flush: before every call into the core that may move its day (a report,
//! a grant, a way out, a decision that keeps a warning), when the app
//! changes, when counting stops and when the clock jumps. Decisions are made
//! every call on a copy of the state with the stretch recorded, so they are
//! exactly what they would be had every second been recorded.

use limo_core::{Decision, DeviceState, LockReason, Slice};
use serde::{Deserialize, Serialize};

use crate::error::LoadProblem;
use crate::facts::{Permission, PhoneFacts, PhonePermissions};
use crate::orders::PhoneNote;
use crate::{CLOCK_JUMP_MS, MAX_APPS_A_DAY, MAX_COUNTED_STEP_MS, MAX_NOTES_KEPT, NOT_WATCHED_NOTE_MS, STATE_FORMAT};

/// The first line of every kept state.
const MAGIC: &str = "limo-phone-state";

/// A moment: LiMo's clock, and the elapsed count on one start of the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Moment {
    /// LiMo's clock, unix seconds.
    pub(crate) wall: i64,
    /// The elapsed count, milliseconds.
    pub(crate) elapsed_ms: u64,
    /// The phone's start count.
    pub(crate) boot: u32,
}

/// Use of one app, measured by the elapsed count, not yet recorded in the
/// device state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stretch {
    /// The app's package name; empty when the host could not name it.
    app: String,
    /// Milliseconds of use not yet recorded. A flush records the whole
    /// seconds; the part of a second stays here for more use of the same
    /// app.
    ms: u64,
    /// LiMo's clock at the last step counted: what a flush records ends
    /// here.
    wall_end: i64,
}

/// The last look at which the child's time was being counted. The next
/// step is counted from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Look {
    boot: u32,
    elapsed_ms: u64,
    wall: i64,
    /// The app in use then: the next step is its use.
    app: String,
    /// The most milliseconds the next step may count: the time in hand
    /// then, less the part of a second already counted. None when there is
    /// no such limit (a day with no limit, an unlock, an app that does not
    /// count).
    cap_ms: Option<u64>,
}

/// LiMo's clock, as the last call saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClockSeen {
    boot: u32,
    elapsed_ms: u64,
    /// The phone's wall clock then, unix seconds.
    wall: i64,
    /// How far the wall clock is ahead of LiMo's clock, in seconds: the
    /// jumps not followed. 0 while automatic time is on.
    correction_secs: i64,
    utc_offset_secs: i32,
}

/// A blocked app noted for a day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BlockedNoted {
    day: i64,
    package: String,
}

/// One app's recorded use on one day, whether its time counts or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AppUse {
    /// Its package name.
    pub(crate) package: String,
    /// Seconds recorded.
    pub(crate) secs: u64,
}

/// The use of each app on one home day: what Today lists. Kept with the
/// device state and changed only where the device state records use, so
/// the two always agree. It holds one day at a time: a slice of a later day
/// empties it and starts that day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AppsDay {
    /// The home day, as the core put it on the slices.
    day: i64,
    /// At most [`MAX_APPS_A_DAY`].
    apps: Vec<AppUse>,
}

impl AppsDay {
    /// Adds the slices the device state made: each to its app on its day.
    /// The app the host could not name (an empty package) is left out.
    fn add(day: &mut Option<AppsDay>, slices: &[Slice]) {
        for slice in slices.iter().filter(|s| !s.app_id.is_empty()) {
            let open = match day.take() {
                Some(d) if d.day >= slice.day => d,
                _ => AppsDay { day: slice.day, apps: Vec::new() },
            };
            let open = day.insert(open);
            // The device's day never moves back, so a slice of an earlier
            // day cannot come; were one to, it is not today's.
            if open.day != slice.day {
                continue;
            }
            if let Some(app) = open.apps.iter_mut().find(|a| a.package == slice.app_id) {
                app.secs = app.secs.saturating_add(slice.secs());
                continue;
            }
            if open.apps.len() >= MAX_APPS_A_DAY {
                // Full: the app with the least time makes room.
                let least = open.apps.iter().enumerate().min_by_key(|(_, a)| a.secs).map(|(i, _)| i);
                if let Some(least) = least {
                    open.apps.remove(least);
                }
            }
            open.apps.push(AppUse { package: slice.app_id.clone(), secs: slice.secs() });
        }
    }
}

/// Whether a guardian has finished setup on this phone. Until then the
/// engine counts nothing and covers nothing: the guardian is holding the
/// phone, on Android's own pages, and that is not the child's time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Setup {
    /// Paired, and setup not finished yet.
    NotFinished,
    /// A guardian finished it, at this moment by LiMo's clock.
    Finished {
        /// When.
        at: i64,
    },
    /// Kept by an engine that did not know of setup: its phone was paired
    /// and in use already, so it counts as finished. Taking it as not
    /// finished would stop a phone in use from keeping its limits.
    KeptBefore,
}

impl Setup {
    /// What a kept state with no word of setup holds.
    fn kept_before() -> Setup {
        Setup::KeptBefore
    }
}

/// Everything the phone keeps, in one blob. Every field added after the
/// first form is filled in when missing, so a state kept by an earlier
/// version loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Kept {
    /// The rules core's device state: what the phone holds, its slices, its
    /// code secret and PIN record, its wrong guesses, its requests.
    pub(crate) state: DeviceState,
    #[serde(default)]
    stretch: Option<Stretch>,
    #[serde(default)]
    look: Option<Look>,
    #[serde(default)]
    clock: Option<ClockSeen>,
    /// Notes for the guardian not yet taken by the master, oldest first.
    #[serde(default)]
    pub(crate) notes: Vec<PhoneNote>,
    #[serde(default)]
    permissions: Option<PhonePermissions>,
    #[serde(default)]
    blocked_noted: Vec<BlockedNoted>,
    /// Each app's use on the phone's day, for Today.
    #[serde(default)]
    apps_day: Option<AppsDay>,
    /// Whether setup is finished. A new pairing starts not finished; a state
    /// kept before this field was there is taken as finished (see
    /// [`Setup::KeptBefore`]).
    #[serde(default = "Setup::kept_before")]
    setup: Setup,
}

/// How a kept state differs from the one last saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// Not at all, but for the clock's last reading.
    Nothing,
    /// Only counting going on as before: the same app's stretch grown, a
    /// later look at the same app.
    Progress,
    /// Anything else.
    Other,
}

/// FNV-1a, 64 bits: a check that a kept state was not cut short or changed
/// by accident. It is not a defence against anyone who can write the file.
fn check(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Kept {
    /// A new kept state, from the device state made at pairing: setup is
    /// not finished yet.
    pub(crate) fn new(state: DeviceState) -> Kept {
        Kept {
            state,
            stretch: None,
            look: None,
            clock: None,
            notes: Vec::new(),
            permissions: None,
            blocked_noted: Vec::new(),
            apps_day: None,
            setup: Setup::NotFinished,
        }
    }

    /// True once setup is finished on this phone.
    pub(crate) fn setup_finished(&self) -> bool {
        self.setup != Setup::NotFinished
    }

    /// When setup was finished, by LiMo's clock; None while it is not, and
    /// for a state kept before setup was kept.
    pub(crate) fn setup_finished_at(&self) -> Option<i64> {
        match self.setup {
            Setup::Finished { at } => Some(at),
            Setup::NotFinished | Setup::KeptBefore => None,
        }
    }

    /// A guardian finished setup at `at` (LiMo's clock): noted for the
    /// guardian, once. True when this finished it, false when it was
    /// finished already, which changes nothing.
    pub(crate) fn finish_setup(&mut self, at: i64) -> bool {
        if self.setup_finished() {
            return false;
        }
        self.setup = Setup::Finished { at };
        self.keep_notes(&[PhoneNote::SetupFinished { at }]);
        true
    }

    /// The blob the host writes.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let body = serde_json::to_string(self).unwrap_or_default();
        format!("{MAGIC}\nformat {STATE_FORMAT}\ncheck {:016x}\n{body}", check(body.as_bytes())).into_bytes()
    }

    /// A kept state from the blob the host read.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Kept, LoadProblem> {
        if bytes.is_empty() {
            return Err(LoadProblem::Empty);
        }
        let head = format!("{MAGIC}\n");
        if !bytes.starts_with(head.as_bytes()) {
            return Err(LoadProblem::NotLiMo);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| LoadProblem::Damaged)?;
        let mut lines = text.splitn(4, '\n').skip(1);
        let format = lines
            .next()
            .and_then(|l| l.strip_prefix("format "))
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or(LoadProblem::Damaged)?;
        if format > STATE_FORMAT {
            return Err(LoadProblem::Newer { format });
        }
        if format == 0 {
            return Err(LoadProblem::Damaged);
        }
        let sum = lines
            .next()
            .and_then(|l| l.strip_prefix("check "))
            .filter(|h| h.len() == 16)
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .ok_or(LoadProblem::Damaged)?;
        let body = lines.next().ok_or(LoadProblem::Damaged)?;
        if check(body.as_bytes()) != sum {
            return Err(LoadProblem::Damaged);
        }
        let kept: Kept = serde_json::from_str(body).map_err(|_| LoadProblem::Damaged)?;
        if kept.state.child_id().is_empty() || kept.state.device_id().is_empty() {
            return Err(LoadProblem::Damaged);
        }
        Ok(kept)
    }

    /// How this differs from `saved`.
    pub(crate) fn change_since(&self, saved: &Kept) -> Change {
        let clock = |k: &Kept| k.clock.map(|c| (c.boot, c.correction_secs, c.utc_offset_secs));
        if self.state != saved.state
            || self.notes != saved.notes
            || self.permissions != saved.permissions
            || self.blocked_noted != saved.blocked_noted
            || self.apps_day != saved.apps_day
            || self.setup != saved.setup
            || clock(self) != clock(saved)
        {
            return Change::Other;
        }
        if self.stretch == saved.stretch && self.look == saved.look {
            return Change::Nothing;
        }
        let stretch = match (&saved.stretch, &self.stretch) {
            (Some(a), Some(b)) => a.app == b.app,
            (None, None) => true,
            _ => false,
        };
        let look = match (&saved.look, &self.look) {
            (Some(a), Some(b)) => a.boot == b.boot && a.app == b.app,
            _ => false,
        };
        if stretch && look { Change::Progress } else { Change::Other }
    }

    /// LiMo's clock now, from the facts, with notes for what moved since the
    /// last call: the wall clock against the elapsed count, the time zone, a
    /// restart of the phone.
    ///
    /// While automatic time is on, the network sets the wall clock and
    /// nobody can set it by hand: LiMo follows it. While it is off, a jump
    /// is somebody setting it: LiMo keeps its own clock, run on by the
    /// elapsed count, and the jump buys nothing. Across a restart of the
    /// phone the elapsed count starts again, so LiMo keeps the correction it
    /// had, and only makes sure its clock does not go back.
    pub(crate) fn watch_clock(&mut self, facts: &PhoneFacts, notes: &mut Vec<PhoneNote>) -> i64 {
        let mut correction = 0;
        let mut moved = None;
        if let Some(prev) = self.clock {
            correction = prev.correction_secs;
            if prev.boot != facts.boot_count {
                notes.push(PhoneNote::Restarted { at: facts.wall_secs.saturating_sub(correction) });
                // The phone was off for a while: its clock cannot be checked
                // against the elapsed count, but it cannot have gone back.
                let before = prev.wall.saturating_sub(prev.correction_secs);
                let now = facts.wall_secs.saturating_sub(correction);
                if now < before {
                    moved = Some(now.saturating_sub(before));
                }
            } else if let Some(step) = facts.elapsed_ms.checked_sub(prev.elapsed_ms) {
                let wall_ms = facts.wall_secs.saturating_sub(prev.wall).saturating_mul(1_000);
                let off = wall_ms.saturating_sub(i64::try_from(step).unwrap_or(i64::MAX));
                if off.unsigned_abs() > CLOCK_JUMP_MS {
                    let by = if off >= 0 { off.saturating_add(500) / 1_000 } else { off.saturating_sub(500) / 1_000 };
                    moved = Some(by);
                }
            }
            if prev.utc_offset_secs != facts.utc_offset_secs {
                notes.push(PhoneNote::TimeZoneChanged {
                    from_offset_secs: prev.utc_offset_secs,
                    to_offset_secs: facts.utc_offset_secs,
                    at: facts.wall_secs.saturating_sub(correction),
                });
            }
        }
        if let Some(by) = moved
            && !facts.auto_time
        {
            correction = correction.saturating_add(by);
        }
        if facts.auto_time {
            correction = 0;
        }
        let now = facts.wall_secs.saturating_sub(correction);
        if let Some(by_secs) = moved {
            notes.push(PhoneNote::ClockMoved { by_secs, followed: facts.auto_time, at: now });
        }
        self.clock = Some(ClockSeen {
            boot: facts.boot_count,
            elapsed_ms: facts.elapsed_ms,
            wall: facts.wall_secs,
            correction_secs: correction,
            utc_offset_secs: facts.utc_offset_secs,
        });
        now
    }

    /// LiMo's clock for a wall clock reading, as the last call corrected it.
    pub(crate) fn limo_time(&self, wall_secs: i64) -> i64 {
        wall_secs.saturating_sub(self.clock.map_or(0, |c| c.correction_secs))
    }

    /// How far the wall clock is ahead of LiMo's clock, in seconds.
    pub(crate) fn correction_secs(&self) -> i64 {
        self.clock.map_or(0, |c| c.correction_secs)
    }

    /// Notes each permission that was on and is now off, in the order setup
    /// asks for them. Not while setup is not finished: the guardian is
    /// switching them on then, and Android may say one is on a moment
    /// before it is; what is seen is kept all the same, so that a
    /// permission switched off after setup is noted.
    pub(crate) fn watch_permissions(&mut self, now: PhonePermissions, at: i64, notes: &mut Vec<PhoneNote>) {
        if let Some(before) = self.permissions.filter(|_| self.setup_finished()) {
            for permission in Permission::ALL {
                if before.has(permission) && !now.has(permission) {
                    notes.push(PhoneNote::PermissionOff { permission, at });
                }
            }
        }
        self.permissions = Some(now);
    }

    /// The permissions the last call saw, if any call did.
    pub(crate) fn permissions(&self) -> Option<PhonePermissions> {
        self.permissions
    }

    /// LiMo's clock at the last call, and the phone's time zone then, if any
    /// call was made.
    pub(crate) fn last_seen(&self) -> Option<(i64, i32)> {
        self.clock.map(|c| (c.wall.saturating_sub(c.correction_secs), c.utc_offset_secs))
    }

    /// Notes a blocked app opened, once a day for each app.
    pub(crate) fn note_blocked(&mut self, package: &str, day: i64, at: i64, notes: &mut Vec<PhoneNote>) {
        self.blocked_noted.retain(|b| b.day >= day);
        if self.blocked_noted.iter().any(|b| b.day == day && b.package == package) {
            return;
        }
        self.blocked_noted.push(BlockedNoted { day, package: package.to_string() });
        notes.push(PhoneNote::BlockedAppOpened { package: package.to_string(), day, at });
    }

    /// Keeps new notes, the newest [`MAX_NOTES_KEPT`] of all.
    pub(crate) fn keep_notes(&mut self, new: &[PhoneNote]) {
        self.notes.extend_from_slice(new);
        let over = self.notes.len().saturating_sub(MAX_NOTES_KEPT);
        self.notes.drain(..over);
    }

    /// Counts the step from the last look to `at`. Returns the milliseconds
    /// counted.
    pub(crate) fn take_step(&mut self, at: Moment, notes: &mut Vec<PhoneNote>) -> u64 {
        // The look is used once: any save from here on has none, so a
        // restart in the middle of this call can lose this step but never
        // count it twice.
        let Some(look) = self.look.take() else { return 0 };
        if look.boot != at.boot {
            self.flush();
            return 0;
        }
        let Some(step) = at.elapsed_ms.checked_sub(look.elapsed_ms) else {
            self.flush();
            return 0;
        };
        if step > MAX_COUNTED_STEP_MS {
            self.flush();
            if step >= NOT_WATCHED_NOTE_MS {
                notes.push(PhoneNote::NotWatched { from: look.wall, secs: step / 1_000 });
            }
            return 0;
        }
        // LiMo's clock jumped within the step (automatic time is on, and the
        // network set it): what is open is recorded where it was, and the
        // step, measured by the elapsed count, starts anew at the new time.
        let wall_ms = at.wall.saturating_sub(look.wall).saturating_mul(1_000);
        let off = wall_ms.saturating_sub(i64::try_from(step).unwrap_or(i64::MAX));
        if off.unsigned_abs() > CLOCK_JUMP_MS {
            self.flush();
        }
        let ms = look.cap_ms.map_or(step, |cap| step.min(cap));
        if ms > 0 {
            match &mut self.stretch {
                Some(open) if open.app == look.app => {
                    open.ms = open.ms.saturating_add(ms);
                    open.wall_end = at.wall;
                }
                _ => {
                    self.flush();
                    self.stretch = Some(Stretch { app: look.app.clone(), ms, wall_end: at.wall });
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
        if let Some(open) = &mut self.stretch {
            let secs = open.ms / 1_000;
            if secs > 0 {
                let start = open.wall_end.saturating_sub(i64::try_from(secs).unwrap_or(i64::MAX));
                let made = self.state.record_use(&open.app, start, open.wall_end);
                AppsDay::add(&mut self.apps_day, &made);
                open.ms %= 1_000;
            }
        }
    }

    /// A copy of the device state with the open stretch recorded, and the
    /// slices that made.
    fn probe_parts(&self) -> (DeviceState, Vec<Slice>) {
        let mut probe = self.state.clone();
        let mut made = Vec::new();
        if let Some(open) = &self.stretch {
            let secs = open.ms / 1_000;
            if secs > 0 {
                let start = open.wall_end.saturating_sub(i64::try_from(secs).unwrap_or(i64::MAX));
                made = probe.record_use(&open.app, start, open.wall_end);
            }
        }
        (probe, made)
    }

    /// A copy of the device state with the open stretch recorded.
    pub(crate) fn probe_state(&self) -> DeviceState {
        self.probe_parts().0
    }

    /// Each app's use on home day `day`, the open stretch included, as it was
    /// recorded: in no particular order, apps of any length.
    pub(crate) fn apps_on(&self, day: i64) -> Vec<AppUse> {
        let mut apps = self.apps_day.clone();
        AppsDay::add(&mut apps, &self.probe_parts().1);
        apps.filter(|a| a.day == day).map(|a| a.apps).unwrap_or_default()
    }

    /// The decision as it stands, without changing anything kept.
    pub(crate) fn probe(&self, now: i64, app: Option<&str>) -> Decision {
        self.probe_state().decide(now, app)
    }

    /// The decision behind the orders, from its probe: when the probe gives
    /// a warning, or otherwise changes the warnings already given, the
    /// stretch is recorded and the device state decides itself, so that it
    /// keeps them and each warning comes once. A decision made without this
    /// (nobody can read a warning now) gives and keeps no warning: it comes
    /// when the screen is in use again, with the time actually left.
    pub(crate) fn keep_warnings(&mut self, now: i64, app: Option<&str>, probe: Decision) -> Decision {
        if probe.warned != self.state.holding_at(now).warned {
            self.flush();
            return self.state.decide(now, app);
        }
        probe
    }

    /// The lock that would come at `lock_at` if use went on from `now`
    /// without a break: what a warning that the time runs out is about.
    pub(crate) fn coming_lock(&self, now: i64, lock_at: i64) -> Option<LockReason> {
        let mut probe = self.probe_state();
        if lock_at > now {
            probe.record_use("", now, lock_at);
        }
        probe.decide(lock_at, None).device_lock.map(|l| l.reason)
    }

    /// Ends this call's look: the child is using `app` now and its time is
    /// being counted, or, with `None`, it is not. What is open for another
    /// app, or when counting stops, is recorded.
    pub(crate) fn set_look(&mut self, at: Moment, app: Option<&str>, decision: &Decision) {
        let look = app.map(|app| {
            let part = self.stretch.as_ref().filter(|s| s.app == app).map_or(0, |s| s.ms % 1_000);
            let cap_ms = match (decision.seconds_in_hand, decision.unlocked_until) {
                (Some(secs), None) if decision.counts => Some(secs.saturating_mul(1_000).saturating_sub(part)),
                _ => None,
            };
            Look { boot: at.boot, elapsed_ms: at.elapsed_ms, wall: at.wall, app: app.to_string(), cap_ms }
        });
        let goes_on = match (&look, &self.stretch) {
            (Some(look), Some(open)) => look.app == open.app,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if !goes_on {
            self.flush();
        }
        self.look = look;
    }

    /// True when the phone has use or added time the master has not
    /// confirmed, or use not yet recorded.
    pub(crate) fn has_unreported(&self) -> bool {
        !self.state.unconfirmed_slices().is_empty()
            || !self.state.unsent_added().is_empty()
            || self.stretch.as_ref().is_some_and(|s| s.ms >= 1_000)
    }

    /// Whole seconds of use not yet recorded in the device state.
    pub(crate) fn open_secs(&self) -> u64 {
        self.stretch.as_ref().map_or(0, |s| s.ms / 1_000)
    }

    /// The slices the master has not confirmed, with the open stretch as
    /// one more.
    pub(crate) fn unreported(&self) -> Vec<Slice> {
        self.probe_state().unconfirmed_slices()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_core::{DeviceKind, MasterBook, RuleSet};

    fn kept() -> Kept {
        let mut book = MasterBook::new("child-1", RuleSet::new(1, 19_800)).unwrap();
        let welcome = book.add_device("phone-1", DeviceKind::Portable, 1_800_000_000).unwrap();
        Kept::new(DeviceState::from_pairing(&welcome, 1_800_000_000))
    }

    #[test]
    fn the_check_catches_a_change_anywhere_in_the_body() {
        let k = kept();
        let bytes = k.encode();
        assert_eq!(Kept::decode(&bytes), Ok(k.clone()));
        let body_start = bytes.iter().position(|b| *b == b'{').unwrap();
        for i in body_start..bytes.len() {
            let mut changed = bytes.clone();
            changed[i] ^= 0x04;
            assert!(Kept::decode(&changed).is_err(), "a change at {i} was not seen");
        }
        assert_eq!(check(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(check(b"a"), 0xaf63_dc4c_8601_ec8c, "the FNV-1a known answer for one letter");
    }

    #[test]
    fn notes_are_kept_up_to_the_limit_newest_last() {
        let mut k = kept();
        let notes: Vec<PhoneNote> = (0..250).map(|i| PhoneNote::Restarted { at: i }).collect();
        k.keep_notes(&notes);
        assert_eq!(k.notes.len(), MAX_NOTES_KEPT);
        assert_eq!(k.notes.first(), Some(&PhoneNote::Restarted { at: 50 }));
        assert_eq!(k.notes.last(), Some(&PhoneNote::Restarted { at: 249 }));
    }

    fn slice(app: &str, day: i64, secs: i64) -> Slice {
        Slice {
            id: format!("p/{app}/{day}/{secs}"),
            child_id: "c".into(),
            device_id: "p".into(),
            app_id: app.into(),
            day,
            start: 0,
            end: secs,
            counts: true,
        }
    }

    #[test]
    fn each_apps_use_is_added_up_for_one_day_bounded_and_starts_again_with_the_next() {
        let mut apps = None;
        AppsDay::add(&mut apps, &[slice("org.example.maps", 5, 600), slice("org.example.game", 5, 90)]);
        AppsDay::add(&mut apps, &[slice("org.example.maps", 5, 120), slice("", 5, 300)]);
        let today = apps.clone().unwrap();
        assert_eq!(today.day, 5);
        assert_eq!(
            today.apps,
            vec![
                AppUse { package: "org.example.maps".into(), secs: 720 },
                AppUse { package: "org.example.game".into(), secs: 90 },
            ],
            "the app the host could not name is left out"
        );
        // A slice of an earlier day changes nothing; one of a later day
        // starts that day afresh.
        AppsDay::add(&mut apps, &[slice("org.example.old", 4, 60)]);
        assert_eq!(apps, Some(today));
        AppsDay::add(&mut apps, &[slice("org.example.game", 6, 30)]);
        assert_eq!(apps, Some(AppsDay { day: 6, apps: vec![AppUse { package: "org.example.game".into(), secs: 30 }] }));
        // Bounded: a new app beyond the most takes the place of the least.
        let many: Vec<Slice> = (0..MAX_APPS_A_DAY as i64).map(|i| slice(&format!("org.example.a{i}"), 7, 100 + i)).collect();
        AppsDay::add(&mut apps, &many);
        AppsDay::add(&mut apps, &[slice("org.example.new", 7, 5_000)]);
        let full = apps.unwrap();
        assert_eq!(full.apps.len(), MAX_APPS_A_DAY);
        assert!(full.apps.iter().all(|a| a.package != "org.example.a0"), "the least went");
        assert!(full.apps.iter().any(|a| a.package == "org.example.new" && a.secs == 5_000));
    }

    #[test]
    fn counting_going_on_is_progress_and_anything_else_a_change() {
        let mut k = kept();
        let saved = k.clone();
        assert_eq!(k.change_since(&saved), Change::Nothing);
        let at = Moment { wall: 1_800_000_100, elapsed_ms: 100_000, boot: 3 };
        let d = k.probe(at.wall, Some("org.example.game"));
        k.set_look(at, Some("org.example.game"), &d);
        assert_eq!(k.change_since(&saved), Change::Other);
        let saved = k.clone();
        let later = Moment { wall: at.wall + 2, elapsed_ms: at.elapsed_ms + 2_000, boot: 3 };
        assert_eq!(k.take_step(later, &mut Vec::new()), 2_000);
        k.set_look(later, Some("org.example.game"), &d);
        assert_eq!(k.change_since(&saved), Change::Other, "a stretch started");
        let saved = k.clone();
        let later = Moment { wall: later.wall + 1, elapsed_ms: later.elapsed_ms + 1_000, boot: 3 };
        k.take_step(later, &mut Vec::new());
        k.set_look(later, Some("org.example.game"), &d);
        assert_eq!(k.change_since(&saved), Change::Progress);
        k.set_look(later, Some("org.example.other"), &d);
        assert_eq!(k.change_since(&saved), Change::Other, "another app");
    }
}
