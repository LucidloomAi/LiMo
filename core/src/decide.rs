//! The decision on a device: may this child use this device now, for how
//! long, and if not, why not and until when.
//!
//! [`decide`] is a pure function of the rules the device holds, what it
//! holds for the day (a [`Holding`]), the time now and the app in front. Its
//! answer, a [`Decision`], is all a Windows or Android service needs to act:
//! it never has to look at a rule itself.

use serde::{Deserialize, Serialize};

use crate::blocked::{BlockedHours, LOOK_AHEAD_SECS};
use crate::rules::RuleSet;

/// While locked because no time is in hand, how often to ask again, in
/// seconds: more time can come from the master at any moment.
pub const RETRY_SECS: i64 = 60;

/// Seconds as an `i64`, without wrapping.
fn secs_i64(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// What a device holds for one home day: the time it may spend without
/// asking anyone, and what it has spent.
///
/// The time in hand is the lease left plus the away allowance left. The lease
/// comes from the master in small pieces while the device is in reach. The
/// away allowance is for use out of reach; it is spent only when the lease is
/// gone. Spending takes from the lease first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    /// The home day these figures are for.
    pub day: i64,
    /// All the lease this device has been handed for the day, in seconds.
    pub lease_total_secs: u64,
    /// The part of it already spent.
    pub lease_used_secs: u64,
    /// The away allowance for the day: the most this device may spend out of
    /// reach of the master in the whole day, in seconds.
    pub away_cap_secs: u64,
    /// Time spent from the away allowance today. It can pass the cap when the
    /// cap was lowered after the time was spent.
    pub away_used_secs: u64,
    /// The away share, in percent of a day's budget, this device uses to work
    /// out its own away allowance when a new day starts. Set by the master; 0
    /// on the master PC.
    pub away_percent: u8,
    /// What the master last said about the time left for this device today,
    /// or None.
    pub master_word: Option<MasterWord>,
    /// The warnings already given.
    pub warned: Warned,
}

/// What the master said, with its last grant, about the time left today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterWord {
    /// Seconds this device could still use today, counting what it holds and
    /// what the master had not handed out to anyone, if no other device used
    /// any.
    pub left_secs: u64,
    /// This device's use today, in seconds, when the master said it.
    pub used_then_secs: u64,
    /// Away time other devices held then and had not used: time that may
    /// still come to this device when they report from home.
    #[serde(default)]
    pub held_elsewhere_secs: u64,
}

/// The warnings already given, so that each warning point is given once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warned {
    /// The home day the time warnings below belong to.
    pub time_day: Option<i64>,
    /// Warning points, in minutes, already given for the time running out.
    pub time_points: Vec<u32>,
    /// The start of the blocked hours the warnings below belong to.
    pub blocked_start: Option<i64>,
    /// Warning points, in minutes, already given for those blocked hours.
    pub blocked_points: Vec<u32>,
}

impl Holding {
    /// What a device holds at the start of home day `day` before it hears
    /// from the master: no lease, and `away_percent` of the day's budget as
    /// its away allowance.
    pub fn fresh(rules: &RuleSet, day: i64, away_percent: u8) -> Holding {
        Holding {
            day,
            lease_total_secs: 0,
            lease_used_secs: 0,
            away_cap_secs: rules.away_cap_secs(away_percent, day),
            away_used_secs: 0,
            away_percent,
            master_word: None,
            warned: Warned::default(),
        }
    }

    /// Lease not yet spent, in seconds.
    pub fn lease_left_secs(&self) -> u64 {
        self.lease_total_secs.saturating_sub(self.lease_used_secs)
    }

    /// Away allowance not yet spent, in seconds.
    pub fn away_left_secs(&self) -> u64 {
        self.away_cap_secs.saturating_sub(self.away_used_secs)
    }

    /// Time this device may still spend today without asking anyone.
    pub fn in_hand_secs(&self) -> u64 {
        self.lease_left_secs().saturating_add(self.away_left_secs())
    }

    /// Time that counts which this device has spent today.
    pub fn used_secs(&self) -> u64 {
        self.lease_used_secs.saturating_add(self.away_used_secs)
    }

    /// This holding as it stands on home day `day`. On a later day it is a
    /// fresh holding (see [`Holding::fresh`]) that keeps the blocked hours
    /// warnings. A device's day never moves back: asked about an earlier day
    /// (a clock set back, or new rules that move the day start later or the
    /// home zone back), it stays as it is, and use keeps counting on its day.
    pub fn for_day(&self, rules: &RuleSet, day: i64) -> Holding {
        if day <= self.day {
            return self.clone();
        }
        let mut next = Holding::fresh(rules, day, self.away_percent);
        next.warned.blocked_start = self.warned.blocked_start;
        next.warned.blocked_points = self.warned.blocked_points.clone();
        next
    }

    /// Spends `secs` of counted time: from the lease first, then from the
    /// away allowance. Use past the allowance is still recorded.
    pub fn spend(&mut self, secs: u64) {
        let from_lease = secs.min(self.lease_left_secs());
        self.lease_used_secs = self.lease_used_secs.saturating_add(from_lease);
        self.away_used_secs = self.away_used_secs.saturating_add(secs - from_lease);
    }

    /// The master's word, brought up to date with this device's use since.
    /// It is trusted only while some lease is left: a device in reach asks
    /// again before its lease runs out, so a spent lease means the word may
    /// be old.
    fn master_estimate(&self) -> Option<u64> {
        let word = self.master_word.as_ref()?;
        if self.lease_left_secs() == 0 {
            return None;
        }
        let since = self.used_secs().saturating_sub(word.used_then_secs);
        Some(word.left_secs.saturating_sub(since))
    }

    /// The best known time left today: the master's word when it can be
    /// trusted, but never less than the time in hand.
    pub fn best_estimate_secs(&self) -> u64 {
        self.in_hand_secs().max(self.master_estimate().unwrap_or(0))
    }

    /// True when the master's last word says there was nothing beyond what
    /// this device held.
    fn master_said_nothing_more(&self) -> bool {
        match &self.master_word {
            Some(word) => word.left_secs <= self.used_secs().saturating_sub(word.used_then_secs),
            None => false,
        }
    }
}

/// Why something cannot be used now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockReason {
    /// This app is blocked. It can never be used.
    AppBlocked,
    /// It is blocked hours, such as bedtime.
    BlockedHours,
    /// Today has no screen time at all (a budget of 0).
    NoTimeToday,
    /// Today's time is used up.
    TimeUsedUp,
    /// This device has spent the time it holds, and as far as it knows there
    /// is more today at the master. More comes when it is in reach of the
    /// master again.
    NeedsMaster,
    /// This device has spent the time it holds and the master has none free,
    /// but another device, such as a phone, still holds away time today. It
    /// comes here when that device reports from home.
    HeldByAnotherDevice,
}

/// A lock, with its reason and when it lifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// Why.
    pub reason: LockReason,
    /// When use is possible again if nothing changes (no extra time, no new
    /// rules), in unix seconds. None when not within the next eight days, or
    /// never (a blocked app).
    pub until: Option<i64>,
}

/// What a warning is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCause {
    /// Today's time is running out.
    TimeRunsOut,
    /// Blocked hours, such as bedtime, are about to start.
    BlockedHoursStart,
}

/// A warning that a lock is coming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// What the lock will be for.
    pub cause: WarningCause,
    /// The warning point reached, in minutes: 15, 5 or 1 with the default
    /// rules.
    pub minutes: u32,
    /// When the lock is expected, in unix seconds. For time running out this
    /// assumes use goes on without a break.
    pub lock_at: i64,
}

/// The answer for a device at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The home day of the moment asked about.
    pub day: i64,
    /// True when the app in front may be used now. With no app given, true
    /// when the device may be used for ordinary apps.
    pub allowed: bool,
    /// Why not, and until when, when `allowed` is false.
    pub lock: Option<Lock>,
    /// The lock for ordinary apps on this device, whatever app is in front.
    /// The lock screen follows this one; an always allowed app stays usable
    /// under it.
    pub device_lock: Option<Lock>,
    /// True when time with this app in front counts against the budget.
    /// False for an always allowed app.
    pub counts: bool,
    /// The best known time left today in seconds, for showing to the child
    /// and for warnings. None when today has no limit.
    pub seconds_left: Option<u64>,
    /// The time this device may spend without asking anyone, in seconds. The
    /// lock comes when it runs out. None when today has no limit.
    pub seconds_in_hand: Option<u64>,
    /// A warning to show now, when a warning point has just been reached.
    pub warning: Option<Warning>,
    /// True when the device should ask the master for more time now, if it
    /// can reach it: its lease is down to half or less.
    pub ask_master: bool,
    /// When to ask for a decision again, in unix seconds, even if nothing
    /// else happens, while the app in front stays in use: the next warning
    /// point, the lock, the start or end of blocked hours, the lease running
    /// low, or the day changing. Moments that depend on use assume use goes on
    /// without a break; they are left out when the app in front is refused or
    /// does not count. Always later than now. Ask again as well whenever the
    /// app in front changes.
    pub next_check_at: i64,
    /// The same while the device is idle: only the moments that come with the
    /// clock (the day changing, blocked hours starting or ending and their
    /// warnings, a lock lifting, a retry). An idle device can sleep until
    /// this moment or until it is used again, whichever comes first. Always
    /// later than now, and never earlier than `next_check_at`.
    pub next_check_if_idle_at: i64,
    /// The warnings given, this one included. Store it back into the holding.
    pub warned: Warned,
}

/// Decides for a device.
///
/// `rules` are the rules the device holds, `holding` what it holds for its
/// day (it may be for an earlier day: a new day is worked out here), `now`
/// the time in unix seconds and `app` the app in front, if known.
///
/// The day is the home day of `now`, or the holding's day if that is later:
/// a device's day never moves back.
///
/// An always allowed app is allowed in every case and its time does not
/// count. A blocked app is refused in every case.
pub fn decide(rules: &RuleSet, holding: &Holding, now: i64, app: Option<&str>) -> Decision {
    let cal = rules.calendar();
    let today = cal.day_of(now).max(holding.day);
    let held = holding.for_day(rules, today);
    let budget = rules.budget_secs_on(today);
    let blocked = rules.blocked_hours_at(now);
    let in_hand = held.in_hand_secs();
    let estimate = held.best_estimate_secs();

    // The lock for ordinary apps, if any. Blocked hours come first: they are
    // what the child notices.
    let device_reason = if matches!(blocked, BlockedHours::Blocked { .. }) {
        Some(LockReason::BlockedHours)
    } else {
        match budget {
            None => None,
            Some(_) if in_hand > 0 => None,
            Some(0) => Some(LockReason::NoTimeToday),
            Some(secs) if held.used_secs() >= secs => Some(LockReason::TimeUsedUp),
            Some(_) if held.master_said_nothing_more() => {
                if held.master_word.as_ref().is_some_and(|w| w.held_elsewhere_secs > 0) {
                    Some(LockReason::HeldByAnotherDevice)
                } else {
                    Some(LockReason::TimeUsedUp)
                }
            }
            Some(_) => Some(LockReason::NeedsMaster),
        }
    };
    let device_lock = device_reason
        .map(|reason| Lock { reason, until: open_again_at(rules, today, in_hand, now) });

    // The app in front.
    let app_blocked = app.is_some_and(|a| rules.is_blocked_app(a));
    let app_free = !app_blocked && app.is_some_and(|a| rules.is_always_allowed(a));
    let lock = if app_blocked {
        Some(Lock { reason: LockReason::AppBlocked, until: None })
    } else if app_free {
        None
    } else {
        device_lock
    };

    // Warnings, and the moments worth looking again.
    let points = rules.warning_points();
    let mut warned = held.warned.clone();
    if warned.time_day != Some(today) {
        warned.time_day = Some(today);
        warned.time_points.clear();
    }
    let mut warning = None;
    // Moments that come with the clock, and moments that come with use.
    let mut moments = vec![cal.day_start(today.saturating_add(1))];
    let mut use_moments = Vec::new();
    let next_block_start = match blocked {
        BlockedHours::Blocked { until } => {
            moments.extend(until);
            None
        }
        BlockedHours::Open { next_start } => {
            moments.extend(next_start);
            next_start
        }
    };
    if let Some(lock) = &device_lock {
        moments.extend(lock.until);
        if lock.reason != LockReason::BlockedHours {
            moments.push(now.saturating_add(RETRY_SECS));
        }
    } else {
        // Not locked: work out the first lock ahead, assuming use goes on.
        let time_lock_at = budget.map(|_| now.saturating_add(secs_i64(estimate)));
        let first = match (time_lock_at, next_block_start) {
            (Some(t), Some(b)) if b <= t => Some((WarningCause::BlockedHoursStart, b)),
            (Some(t), _) => Some((WarningCause::TimeRunsOut, t)),
            (None, Some(b)) => Some((WarningCause::BlockedHoursStart, b)),
            (None, None) => None,
        };
        if let Some((cause, lock_at)) = first {
            let remaining = lock_at.saturating_sub(now);
            let given = match cause {
                WarningCause::TimeRunsOut => {
                    // More time came (extra time, or better news from the
                    // master): points well clear of the lock may come again.
                    warned.time_points.retain(|p| remaining <= *p as i64 * 60 + RETRY_SECS);
                    &mut warned.time_points
                }
                WarningCause::BlockedHoursStart => {
                    if warned.blocked_start != Some(lock_at) {
                        warned.blocked_start = Some(lock_at);
                        warned.blocked_points.clear();
                    }
                    &mut warned.blocked_points
                }
            };
            let reached: Vec<u32> =
                points.iter().copied().filter(|p| remaining <= *p as i64 * 60).collect();
            let nearest_new = reached.iter().copied().filter(|p| !given.contains(p)).min();
            if let Some(minutes) = nearest_new {
                warning = Some(Warning { cause, minutes, lock_at });
            }
            for p in reached {
                if !given.contains(&p) {
                    given.push(p);
                }
            }
            given.sort_unstable_by(|a, b| b.cmp(a));
        }
        if let Some(start) = next_block_start {
            moments.extend(points.iter().map(|p| start.saturating_sub(*p as i64 * 60)));
        }
        if let Some(lock_at) = time_lock_at {
            use_moments.extend(points.iter().map(|p| lock_at.saturating_sub(*p as i64 * 60)));
            use_moments.push(now.saturating_add(secs_i64(in_hand)));
            let lease_left = secs_i64(held.lease_left_secs());
            let lease_low = secs_i64(rules.lease_secs()) / 2;
            if lease_left > lease_low {
                use_moments.push(now.saturating_add(lease_left - lease_low));
            }
            if lease_left > 0 {
                use_moments.push(now.saturating_add(lease_left));
            }
        }
    }
    let first_after_now = |list: &[i64]| list.iter().copied().filter(|m| *m > now).min();
    let next_check_if_idle_at =
        first_after_now(&moments).unwrap_or(now.saturating_add(RETRY_SECS));
    // Use of the app in front changes nothing when it is refused or does not
    // count; then only the clock moments matter.
    let use_counts = lock.is_none() && !app_free;
    let next_check_at = match first_after_now(&use_moments) {
        Some(m) if use_counts => m.min(next_check_if_idle_at),
        _ => next_check_if_idle_at,
    };

    Decision {
        day: today,
        allowed: lock.is_none(),
        lock,
        device_lock,
        counts: !app_free,
        seconds_left: budget.map(|_| estimate),
        seconds_in_hand: budget.map(|_| in_hand),
        warning,
        ask_master: budget.is_some() && held.lease_left_secs() <= rules.lease_secs() / 2,
        next_check_at,
        next_check_if_idle_at,
        warned,
    }
}

/// When ordinary use is possible again, if nothing changes: the first moment
/// that is outside blocked hours and on a day with time. Today has time only
/// if some is in hand; a later day has time unless its budget is 0. None when
/// that is more than eight days away.
fn open_again_at(rules: &RuleSet, today: i64, in_hand: u64, now: i64) -> Option<i64> {
    let cal = rules.calendar();
    let horizon = now.saturating_add(LOOK_AHEAD_SECS);
    let mut t = now;
    while t <= horizon {
        match rules.blocked_hours_at(t) {
            BlockedHours::Blocked { until: None } => return None,
            BlockedHours::Blocked { until: Some(end) } => {
                t = end;
                continue;
            }
            BlockedHours::Open { .. } => {}
        }
        let day = cal.day_of(t).max(today);
        let has_time = match rules.budget_secs_on(day) {
            None => true,
            Some(_) if day == today => in_hand > 0,
            Some(secs) => day > today && secs > 0,
        };
        if has_time {
            return Some(t);
        }
        let next = cal.day_start(day.saturating_add(1));
        if next <= t {
            return None;
        }
        t = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{BlockedWindow, DailyBudget};
    use crate::time::{CivilDate, Weekday};

    fn rules() -> RuleSet {
        let mut r = RuleSet::new(1, 19_800);
        r.budgets = crate::rules::WeekBudgets::every_day(DailyBudget::Minutes(90));
        r.blocked_hours = vec![BlockedWindow {
            days: Weekday::ALL.to_vec(),
            start_minute: 21 * 60,
            end_minute: 7 * 60,
        }];
        r.always_allowed_apps = vec!["clock".into()];
        r.blocked_apps = vec!["casino".into()];
        r
    }

    fn at(day: u8, hour: u8, minute: u8) -> i64 {
        rules().calendar().at(CivilDate::new(2026, 11, day).unwrap(), hour, minute, 0)
    }

    fn holding(now: i64, lease_left: u64, away_left: u64, used: u64) -> Holding {
        let r = rules();
        Holding {
            lease_total_secs: used + lease_left,
            lease_used_secs: used,
            away_cap_secs: away_left,
            ..Holding::fresh(&r, r.calendar().day_of(now), 0)
        }
    }

    #[test]
    fn the_brief_example_locks_at_20_59() {
        let now = at(18, 20, 55);
        let d = decide(&rules(), &holding(now, 240, 0, 86 * 60), now, Some("game"));
        assert!(d.allowed);
        assert_eq!(d.seconds_left, Some(240));
        let w = d.warning.unwrap();
        assert_eq!((w.cause, w.minutes, w.lock_at), (WarningCause::TimeRunsOut, 5, at(18, 20, 59)));
        assert_eq!(d.next_check_at, at(18, 20, 58));
        assert_eq!(d.warned.time_points, vec![15, 5]);
    }

    #[test]
    fn spending_takes_the_lease_first() {
        let mut h = holding(at(18, 10, 0), 300, 600, 0);
        h.spend(400);
        assert_eq!((h.lease_left_secs(), h.away_left_secs(), h.used_secs()), (0, 500, 400));
        h.spend(900);
        assert_eq!((h.away_left_secs(), h.away_used_secs, h.in_hand_secs()), (0, 1_000, 0));
    }

    #[test]
    fn a_new_day_is_worked_out_from_the_rules() {
        let r = rules();
        let old = Holding { away_percent: 50, ..holding(at(17, 20, 0), 0, 0, 90 * 60) };
        let now = at(18, 12, 0);
        let d = decide(&r, &old, now, None);
        assert!(d.allowed);
        assert_eq!(d.seconds_in_hand, Some(45 * 60));
        assert!(d.ask_master);
    }

    #[test]
    fn huge_figures_and_far_times_do_not_panic() {
        let r = rules();
        for now in [i64::MIN / 2, -1, 0, i64::MAX / 2, i64::MAX - 1] {
            let day = r.calendar().day_of(now);
            for (lease_total, lease_used, cap, away_used) in
                [(u64::MAX, 0, u64::MAX, 0), (u64::MAX, u64::MAX, 0, u64::MAX), (0, 0, 0, 0)]
            {
                let h = Holding {
                    day,
                    lease_total_secs: lease_total,
                    lease_used_secs: lease_used,
                    away_cap_secs: cap,
                    away_used_secs: away_used,
                    away_percent: 100,
                    master_word: Some(MasterWord {
                        left_secs: u64::MAX,
                        used_then_secs: 0,
                        held_elsewhere_secs: u64::MAX,
                    }),
                    warned: Warned::default(),
                };
                for app in [None, Some("game"), Some("clock"), Some("casino")] {
                    let d = decide(&r, &h, now, app);
                    assert!(d.next_check_at > now && d.next_check_if_idle_at >= d.next_check_at);
                }
                let mut spent = h.clone();
                spent.spend(u64::MAX);
                assert!(spent.used_secs() >= h.used_secs());
                assert!(spent.in_hand_secs() <= h.in_hand_secs());
            }
        }
    }

    #[test]
    fn next_check_is_always_later_than_now() {
        let r = rules();
        for hour in 0..24 {
            for (lease, away, used) in [(0, 0, 0), (0, 0, 5_400), (600, 0, 0), (60, 30, 100)] {
                let now = at(18, hour, 0);
                let d = decide(&r, &holding(now, lease, away, used), now, None);
                assert!(d.next_check_at > now);
                assert!(d.next_check_if_idle_at >= d.next_check_at);
            }
        }
    }
}
