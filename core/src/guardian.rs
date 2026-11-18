//! What a guardian can do at once, and the wait after wrong guesses.
//!
//! From the master, a guardian can **lock** all of a child's devices now,
//! until a given time or until lifted, or **unlock** them until a given time:
//! blocked hours and a lack of time are lifted, blocked apps stay blocked, and
//! time still counts. Both reach the devices through grants as a
//! [`GuardianSetting`], a level with a version. With the PIN, a guardian can
//! also unlock one device on the spot ([`PinUnlock`]).
//!
//! When more than one of these is in force on a device, the one made last
//! wins until it ends; then one made earlier, if still in force, applies
//! again. Lifting at the master ends everything made before it. See
//! [`Standing::override_at`].
//!
//! Codes and the PIN share one counter of wrong guesses on each device
//! ([`TryCounter`]). After [`FREE_WRONG_GUESSES`] wrong guesses the next try
//! must wait a minute, and each further wrong guess doubles the wait, up to an
//! hour. A right PIN clears the counter; a right code does not.

use serde::{Deserialize, Serialize};

use crate::time::SECONDS_PER_DAY;

/// The longest unlock, from the master or with the PIN: one day ahead.
pub const MAX_UNLOCK_SECS: i64 = SECONDS_PER_DAY;
/// Wrong guesses allowed before the first wait.
pub const FREE_WRONG_GUESSES: u32 = 5;
/// The first wait, in seconds: one minute.
pub const FIRST_WAIT_SECS: i64 = 60;
/// The longest wait, in seconds: one hour.
pub const LONGEST_WAIT_SECS: i64 = 3_600;

/// What the guardian has set at the master for all of a child's devices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianMode {
    /// Nothing: the rules alone decide. Lifting a lock or an unlock sets
    /// this.
    #[default]
    Nothing,
    /// Every device is locked, apart from always allowed apps, until this
    /// moment, or until lifted when None.
    Lock {
        /// Unix seconds, or None for until lifted.
        until: Option<i64>,
    },
    /// Blocked hours and a lack of time are lifted until this moment. Blocked
    /// apps stay blocked, and time still counts against the budget.
    Unlock {
        /// Unix seconds.
        until: i64,
    },
}

/// The guardian's lock or unlock as the master last set it: a level with a
/// version, carried by every grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianSetting {
    /// Grows by one with every change at the master. 0 when nothing was ever
    /// set.
    pub version: u32,
    /// What is set.
    pub mode: GuardianMode,
    /// When it was set, by the master's clock, unix seconds.
    pub set_at: i64,
}

impl Default for GuardianSetting {
    fn default() -> GuardianSetting {
        GuardianSetting { version: 0, mode: GuardianMode::Nothing, set_at: i64::MIN }
    }
}

/// An unlock set with the guardian PIN on one device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinUnlock {
    /// When it ends, unix seconds.
    pub until: i64,
    /// When it was set, by the device's clock, unix seconds.
    pub set_at: i64,
}

/// A lock or an unlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideKind {
    /// Locked by a guardian.
    Lock,
    /// Unlocked by a guardian.
    Unlock,
}

/// Who set an override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideBy {
    /// A guardian at the master.
    Master,
    /// A guardian with the PIN, on this device.
    Pin,
}

/// The guardian's lock or unlock in force on a device at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideInForce {
    /// Lock or unlock.
    pub kind: OverrideKind,
    /// When it ends, or None for a lock until lifted.
    pub until: Option<i64>,
    /// Who set it.
    pub by: OverrideBy,
    /// When it was set, unix seconds.
    pub set_at: i64,
}

/// The wait in seconds after this many wrong guesses: none for the first
/// four, a minute after the fifth, doubling with each one after, at most an
/// hour.
pub fn wait_after_wrong_guesses(wrong: u32) -> i64 {
    if wrong < FREE_WRONG_GUESSES {
        return 0;
    }
    let doublings = wrong - FREE_WRONG_GUESSES;
    if doublings >= 6 {
        return LONGEST_WAIT_SECS;
    }
    (FIRST_WAIT_SECS << doublings).min(LONGEST_WAIT_SECS)
}

/// The wrong guesses on a device, codes and PIN together, and the wait they
/// have set. Kept by the device and saved with it, so it survives restarts.
///
/// The wait is kept as the moment it ends, by the device's clock. A clock set
/// back makes the wait longer, never shorter. A clock set forward shortens
/// it; the platforms lock the clock down for that reason.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TryCounter {
    /// Wrong guesses since the last right PIN.
    pub wrong: u32,
    /// When the wait set by the last wrong guess ends, unix seconds.
    pub wait_until: Option<i64>,
}

impl TryCounter {
    /// The end of the wait in force at `now`, if any.
    pub fn wait_at(&self, now: i64) -> Option<i64> {
        self.wait_until.filter(|w| now < *w)
    }

    /// Counts a wrong guess made at `now`, and sets the wait it calls for. A
    /// wait already set that ends later is kept.
    pub(crate) fn wrong_guess(&mut self, now: i64) {
        self.wrong = self.wrong.saturating_add(1);
        let wait = wait_after_wrong_guesses(self.wrong);
        if wait > 0 {
            let until = now.saturating_add(wait);
            self.wait_until = Some(self.wait_until.map_or(until, |old| old.max(until)));
        }
    }

    /// A right PIN: clears the counter and the wait.
    pub(crate) fn clear(&mut self) {
        *self = TryCounter::default();
    }
}

/// How the guardian's actions and the ways out stand on a device, beside what
/// it holds for the day. [`crate::decide::decide`] takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    /// The newest lock or unlock from the master this device has taken.
    pub from_master: GuardianSetting,
    /// An unlock set with the PIN on this device, if any.
    pub pin_unlock: Option<PinUnlock>,
    /// Wrong guesses of codes and PIN on this device.
    pub wrong_guesses: u32,
    /// When the wait before the next PIN or code try ends, if one was set.
    pub try_wait_until: Option<i64>,
    /// True when the child may ask for more time from this device: no request
    /// of its own waits for an answer today, and it has not asked too often.
    pub may_ask: bool,
    /// True when this device holds a PIN record.
    pub has_pin: bool,
    /// True when this device holds a code secret.
    pub has_code_secret: bool,
}

impl Default for Standing {
    /// Nothing set by a guardian, no wrong guesses, no PIN and no code
    /// secret; the child may ask for time.
    fn default() -> Standing {
        Standing {
            from_master: GuardianSetting::default(),
            pin_unlock: None,
            wrong_guesses: 0,
            try_wait_until: None,
            may_ask: true,
            has_pin: false,
            has_code_secret: false,
        }
    }
}

impl Standing {
    /// The guardian's lock or unlock in force at `t`, if any.
    ///
    /// Of the master's setting and the PIN unlock, those in force at `t` are
    /// looked at, and the one set last wins (the master's on a tie). So a PIN
    /// unlock typed during a lock from the master opens the device until it
    /// ends, and the lock applies again after that if it has not ended; a
    /// lock from the master set after a PIN unlock locks the device until
    /// the lock ends. Lifting at the master ends a PIN unlock set before it.
    /// Times set on different clocks are compared as they are: both clocks
    /// are meant to be right.
    pub fn override_at(&self, t: i64) -> Option<OverrideInForce> {
        let m = &self.from_master;
        let from_master = match m.mode {
            GuardianMode::Lock { until } if until.is_none_or(|u| t < u) => Some(OverrideInForce {
                kind: OverrideKind::Lock,
                until,
                by: OverrideBy::Master,
                set_at: m.set_at,
            }),
            GuardianMode::Unlock { until } if t < until => Some(OverrideInForce {
                kind: OverrideKind::Unlock,
                until: Some(until),
                by: OverrideBy::Master,
                set_at: m.set_at,
            }),
            _ => None,
        };
        let lifted_after =
            |p: &PinUnlock| m.version > 0 && m.mode == GuardianMode::Nothing && m.set_at >= p.set_at;
        let from_pin = self.pin_unlock.filter(|p| t < p.until && !lifted_after(p)).map(|p| OverrideInForce {
            kind: OverrideKind::Unlock,
            until: Some(p.until),
            by: OverrideBy::Pin,
            set_at: p.set_at,
        });
        match (from_master, from_pin) {
            (Some(a), Some(b)) => Some(if b.set_at > a.set_at { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    /// The first moment from `now` on at which no unlock is in force, when
    /// one is in force at `now`: the end of the unlock, or of the next one if
    /// two follow each other without a gap.
    pub fn unlocked_until(&self, now: i64) -> Option<i64> {
        let first = self.override_at(now).filter(|o| o.kind == OverrideKind::Unlock)?;
        let mut t = first.until?;
        for _ in 0..4 {
            match self.override_at(t) {
                Some(OverrideInForce { kind: OverrideKind::Unlock, until: Some(u), .. }) if u > t => t = u,
                _ => break,
            }
        }
        Some(t)
    }

    /// The moments after `now` at which the override in force can change:
    /// the ends of the master's setting and of the PIN unlock.
    pub(crate) fn override_changes_after(&self, now: i64) -> Vec<i64> {
        let mut moments = Vec::new();
        match self.from_master.mode {
            GuardianMode::Lock { until: Some(u) } | GuardianMode::Unlock { until: u } => moments.push(u),
            _ => {}
        }
        moments.extend(self.pin_unlock.map(|p| p.until));
        moments.retain(|m| *m > now);
        moments
    }

    /// The end of the wait before the next PIN or code try, if one is in
    /// force at `now`.
    pub fn try_wait_at(&self, now: i64) -> Option<i64> {
        self.try_wait_until.filter(|w| now < *w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master(version: u32, mode: GuardianMode, set_at: i64) -> GuardianSetting {
        GuardianSetting { version, mode, set_at }
    }

    #[test]
    fn waits_grow_from_the_fifth_wrong_guess_and_stop_at_an_hour() {
        let waits: Vec<i64> = (0..14).map(wait_after_wrong_guesses).collect();
        assert_eq!(waits, vec![0, 0, 0, 0, 0, 60, 120, 240, 480, 960, 1_920, 3_600, 3_600, 3_600]);
        assert_eq!(wait_after_wrong_guesses(u32::MAX), 3_600);
    }

    #[test]
    fn a_clock_set_back_never_shortens_a_wait() {
        let mut c = TryCounter::default();
        for _ in 0..6 {
            c.wrong_guess(10_000);
        }
        assert_eq!(c.wait_at(10_000), Some(10_120));
        // The clock goes back an hour: the same end, so a longer wait.
        assert_eq!(c.wait_at(6_400), Some(10_120));
        // A wrong guess on the set back clock never moves the end earlier.
        c.wrong_guess(5_000);
        assert_eq!(c.wrong, 7);
        assert_eq!(c.wait_until, Some(10_120));
        c.clear();
        assert_eq!(c, TryCounter::default());
    }

    #[test]
    fn the_action_set_last_wins_until_it_ends() {
        let mut s = Standing::default();
        assert_eq!(s.override_at(100), None);
        // A lock from the master until lifted, set at 100.
        s.from_master = master(1, GuardianMode::Lock { until: None }, 100);
        assert_eq!(s.override_at(150).map(|o| o.kind), Some(OverrideKind::Lock));
        // A PIN unlock typed at 200 until 300 opens the device; at 300 the
        // lock applies again.
        s.pin_unlock = Some(PinUnlock { until: 300, set_at: 200 });
        assert_eq!(s.override_at(250).map(|o| (o.kind, o.by)), Some((OverrideKind::Unlock, OverrideBy::Pin)));
        assert_eq!(s.override_at(300).map(|o| o.kind), Some(OverrideKind::Lock));
        assert_eq!(s.unlocked_until(250), Some(300));
        assert_eq!(s.override_changes_after(250), vec![300]);
        // A lock set at the master after the PIN unlock wins over it.
        s.from_master = master(2, GuardianMode::Lock { until: Some(280) }, 260);
        assert_eq!(s.override_at(270).map(|o| o.kind), Some(OverrideKind::Lock));
        assert_eq!(s.override_at(285).map(|o| o.by), Some(OverrideBy::Pin));
        // Lifting at the master ends the PIN unlock set before it.
        s.from_master = master(3, GuardianMode::Nothing, 290);
        assert_eq!(s.override_at(295), None);
        // A PIN unlock set after the lift stands.
        s.pin_unlock = Some(PinUnlock { until: 400, set_at: 350 });
        assert_eq!(s.override_at(360).map(|o| o.by), Some(OverrideBy::Pin));
    }

    #[test]
    fn unlocks_that_follow_each_other_are_one_stretch() {
        let s = Standing {
            from_master: master(1, GuardianMode::Unlock { until: 500 }, 100),
            pin_unlock: Some(PinUnlock { until: 300, set_at: 200 }),
            ..Standing::default()
        };
        assert_eq!(s.unlocked_until(250), Some(500));
        assert_eq!(s.unlocked_until(500), None);
        let mut changes = s.override_changes_after(0);
        changes.sort_unstable();
        assert_eq!(changes, vec![300, 500]);
    }
}
