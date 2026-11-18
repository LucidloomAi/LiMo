//! The rules for one child.
//!
//! A [`RuleSet`] holds everything a device needs to decide on its own: the
//! daily budget for each weekday, the blocked hours, the always allowed and
//! the blocked apps, and the figures that steer the shared budget (the away
//! share, the lease length and the warning points). Each change the guardian
//! makes becomes a new rule set with a higher version number.

use serde::{Deserialize, Serialize};

use crate::time::{HomeCalendar, MINUTES_PER_DAY, Weekday};

/// The daily budget when the guardian has not set one: two hours.
pub const DEFAULT_DAILY_MINUTES: u32 = 120;
/// The away share when the guardian has not set one: half the day's budget.
pub const DEFAULT_AWAY_SHARE_PERCENT: u8 = 50;
/// The lease length when the guardian has not set one: ten minutes.
pub const DEFAULT_LEASE_MINUTES: u32 = 10;
/// The warning points when the guardian has not set any: 15, 5 and 1 minute
/// before a lock.
pub const DEFAULT_WARNING_MINUTES: [u32; 3] = [15, 5, 1];
/// The longest lease the rules allow, in minutes.
pub const MAX_LEASE_MINUTES: u32 = 120;
/// How far the home time zone may be from UTC, in seconds: 14 hours.
pub const MAX_UTC_OFFSET_SECS: i32 = 14 * 3_600;

/// How much screen time one day of the week has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyBudget {
    /// This many minutes. 0 means no screen time that day.
    Minutes(u32),
    /// No limit on time that day. Blocked hours and blocked apps still apply.
    NoLimit,
}

impl DailyBudget {
    /// The budget in seconds, or None when there is no limit.
    pub fn secs(self) -> Option<u64> {
        match self {
            DailyBudget::Minutes(m) => Some(m as u64 * 60),
            DailyBudget::NoLimit => None,
        }
    }
}

/// One daily budget for each day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekBudgets {
    /// Monday's budget.
    pub monday: DailyBudget,
    /// Tuesday's budget.
    pub tuesday: DailyBudget,
    /// Wednesday's budget.
    pub wednesday: DailyBudget,
    /// Thursday's budget.
    pub thursday: DailyBudget,
    /// Friday's budget.
    pub friday: DailyBudget,
    /// Saturday's budget.
    pub saturday: DailyBudget,
    /// Sunday's budget.
    pub sunday: DailyBudget,
}

impl WeekBudgets {
    /// The same budget on all seven days.
    pub fn every_day(budget: DailyBudget) -> WeekBudgets {
        WeekBudgets {
            monday: budget,
            tuesday: budget,
            wednesday: budget,
            thursday: budget,
            friday: budget,
            saturday: budget,
            sunday: budget,
        }
    }

    /// The budget for one weekday.
    pub fn get(&self, day: Weekday) -> DailyBudget {
        match day {
            Weekday::Monday => self.monday,
            Weekday::Tuesday => self.tuesday,
            Weekday::Wednesday => self.wednesday,
            Weekday::Thursday => self.thursday,
            Weekday::Friday => self.friday,
            Weekday::Saturday => self.saturday,
            Weekday::Sunday => self.sunday,
        }
    }

    /// Changes the budget for one weekday.
    pub fn set(&mut self, day: Weekday, budget: DailyBudget) {
        match day {
            Weekday::Monday => self.monday = budget,
            Weekday::Tuesday => self.tuesday = budget,
            Weekday::Wednesday => self.wednesday = budget,
            Weekday::Thursday => self.thursday = budget,
            Weekday::Friday => self.friday = budget,
            Weekday::Saturday => self.saturday = budget,
            Weekday::Sunday => self.sunday = budget,
        }
    }
}

/// Hours in which the device is locked whatever time is left, such as
/// bedtime. Only always allowed apps can be used.
///
/// Times are on the wall clock at home. A window whose end is not after its
/// start runs past midnight into the next date, and belongs to the weekday it
/// starts on: bedtime from 21:00 to 07:00 on Monday covers Monday 21:00 to
/// Tuesday 07:00.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedWindow {
    /// The weekdays on which the window starts.
    pub days: Vec<Weekday>,
    /// The minute of the day it starts, 0 to 1439. 21:00 is 1260.
    pub start_minute: u16,
    /// The minute of the day it ends, 0 to 1440. 1440 is the midnight at the
    /// end of the day. When it is not after the start, the window runs past
    /// midnight.
    pub end_minute: u16,
}

impl BlockedWindow {
    /// How long the window lasts, in minutes: 1 to 1440.
    pub fn length_minutes(&self) -> u32 {
        let start = self.start_minute as u32;
        let end = self.end_minute as u32;
        if end > start { end - start } else { MINUTES_PER_DAY as u32 - start + end }
    }

    /// True when the window runs past midnight into the next date.
    pub fn crosses_midnight(&self) -> bool {
        self.start_minute as u32 + self.length_minutes() > MINUTES_PER_DAY as u32
    }
}

/// The rules for one child, at one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    /// Grows by at least one with every change. Devices keep the highest
    /// version they have seen.
    pub version: u32,
    /// Home time minus UTC, in seconds: the zone of the master PC. Days and
    /// weekdays are counted in this zone on every device, wherever it is.
    pub utc_offset_secs: i32,
    /// The minute of the day at which a new day, with a new budget, starts:
    /// 0 to 1439. 0 is midnight.
    pub day_start_minute: u16,
    /// The daily budget for each weekday.
    pub budgets: WeekBudgets,
    /// Hours in which the device is locked whatever time is left.
    pub blocked_hours: Vec<BlockedWindow>,
    /// Apps that never use up the budget and stay usable when the device is
    /// locked: phone, messages, the clock. App identifiers are opaque.
    pub always_allowed_apps: Vec<String>,
    /// Apps that can never be used.
    pub blocked_apps: Vec<String>,
    /// How much of a day's budget each phone may hold for use out of reach of
    /// the master, as a percent: 0 to 100.
    pub away_share_percent: u8,
    /// How much time a device in reach of the master is handed at a time, in
    /// minutes.
    pub lease_minutes: u32,
    /// How many minutes before a lock a warning is given.
    pub warning_minutes: Vec<u32>,
}

/// One problem found in a rule set, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleProblem {
    /// Which part of the rules, as a path: `budgets.monday`,
    /// `blocked_hours[1].end_minute`. For a screen to point at the field.
    pub field: String,
    /// What is wrong, in plain words, for the guardian.
    pub message: String,
}

impl Default for RuleSet {
    fn default() -> RuleSet {
        RuleSet::new(1, 0)
    }
}

/// "21:00" for minute 1260, "24:00" for minute 1440.
pub fn clock_text(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn offset_text(secs: i32) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let abs = secs.unsigned_abs();
    format!("{sign}{:02}:{:02}", abs / 3_600, abs / 60 % 60)
}

impl RuleSet {
    /// Sensible starting rules: two hours every day, no blocked hours, no
    /// apps listed, half the budget as the away share, ten minute leases,
    /// warnings at 15, 5 and 1 minute, a new day at midnight.
    pub fn new(version: u32, utc_offset_secs: i32) -> RuleSet {
        RuleSet {
            version,
            utc_offset_secs,
            day_start_minute: 0,
            budgets: WeekBudgets::every_day(DailyBudget::Minutes(DEFAULT_DAILY_MINUTES)),
            blocked_hours: Vec::new(),
            always_allowed_apps: Vec::new(),
            blocked_apps: Vec::new(),
            away_share_percent: DEFAULT_AWAY_SHARE_PERCENT,
            lease_minutes: DEFAULT_LEASE_MINUTES,
            warning_minutes: DEFAULT_WARNING_MINUTES.to_vec(),
        }
    }

    /// The home calendar these rules count days in.
    pub fn calendar(&self) -> HomeCalendar {
        HomeCalendar::new(self.utc_offset_secs, self.day_start_minute)
    }

    /// The budget of home day `day`.
    pub fn budget_on(&self, day: i64) -> DailyBudget {
        self.budgets.get(Weekday::of_date(day))
    }

    /// The budget of home day `day` in seconds, or None when it has no limit.
    pub fn budget_secs_on(&self, day: i64) -> Option<u64> {
        self.budget_on(day).secs()
    }

    /// The away allowance a phone with this away percent holds for home day
    /// `day`: that percent of the day's budget, in whole seconds. 0 on a day
    /// with no limit, where it is not needed.
    pub fn away_cap_secs(&self, percent: u8, day: i64) -> u64 {
        match self.budget_secs_on(day) {
            Some(secs) => secs * percent.min(100) as u64 / 100,
            None => 0,
        }
    }

    /// The lease length in seconds.
    pub fn lease_secs(&self) -> u64 {
        self.lease_minutes as u64 * 60
    }

    /// True when `app` never uses up the budget and stays usable when locked.
    pub fn is_always_allowed(&self, app: &str) -> bool {
        self.always_allowed_apps.iter().any(|a| a == app)
    }

    /// True when `app` can never be used.
    pub fn is_blocked_app(&self, app: &str) -> bool {
        self.blocked_apps.iter().any(|a| a == app)
    }

    /// The warning points in minutes, largest first, without zero or repeats.
    pub fn warning_points(&self) -> Vec<u32> {
        let mut points: Vec<u32> = self.warning_minutes.iter().copied().filter(|m| *m > 0).collect();
        points.sort_unstable_by(|a, b| b.cmp(a));
        points.dedup();
        points
    }

    /// True when [`RuleSet::validate`] finds nothing.
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }

    /// Checks the rules and returns every problem found, in plain words. An
    /// empty list means the rules can be used.
    pub fn validate(&self) -> Vec<RuleProblem> {
        let mut problems = Vec::new();
        let mut add = |field: String, message: String| problems.push(RuleProblem { field, message });

        if self.version == 0 {
            add("version".into(), "The version must be 1 or more.".into());
        }
        if self.utc_offset_secs.abs() > MAX_UTC_OFFSET_SECS {
            add(
                "utc_offset_secs".into(),
                format!(
                    "The home time zone is UTC{}, but it must be within 14 hours of UTC.",
                    offset_text(self.utc_offset_secs)
                ),
            );
        } else if self.utc_offset_secs % 60 != 0 {
            add(
                "utc_offset_secs".into(),
                "The home time zone must be a whole number of minutes from UTC.".into(),
            );
        }
        if self.day_start_minute >= MINUTES_PER_DAY {
            add(
                "day_start_minute".into(),
                format!(
                    "The day is set to start at minute {}, but it must start between 00:00 and 23:59.",
                    self.day_start_minute
                ),
            );
        }
        for day in Weekday::ALL {
            if let DailyBudget::Minutes(m) = self.budgets.get(day) {
                if m > MINUTES_PER_DAY as u32 {
                    add(
                        format!("budgets.{}", day.name().to_ascii_lowercase()),
                        format!(
                            "{}'s budget is {m} minutes, more than a whole day. Use no limit instead.",
                            day.name()
                        ),
                    );
                }
            }
        }
        for (i, window) in self.blocked_hours.iter().enumerate() {
            let field = format!("blocked_hours[{i}]");
            let name = format!("Blocked hours number {}", i + 1);
            if window.days.is_empty() {
                add(format!("{field}.days"), format!("{name} has no days."));
            }
            for (j, day) in window.days.iter().enumerate() {
                if window.days[..j].contains(day) {
                    add(format!("{field}.days"), format!("{name} lists {} more than once.", day.name()));
                }
            }
            let start_ok = window.start_minute < MINUTES_PER_DAY;
            let end_ok = window.end_minute <= MINUTES_PER_DAY;
            if !start_ok {
                add(
                    format!("{field}.start_minute"),
                    format!(
                        "{name} starts at minute {}, but it must start between 00:00 and 23:59.",
                        window.start_minute
                    ),
                );
            }
            if !end_ok {
                add(
                    format!("{field}.end_minute"),
                    format!(
                        "{name} ends at minute {}, but it must end between 00:00 and 24:00.",
                        window.end_minute
                    ),
                );
            }
            if start_ok && end_ok && window.start_minute == window.end_minute {
                add(
                    format!("{field}.end_minute"),
                    format!(
                        "{name} starts and ends at {}. To block a whole day, run it from 00:00 to 24:00, or set that day's budget to 0.",
                        clock_text(window.start_minute)
                    ),
                );
            }
        }
        for (list, label, field) in [
            (&self.always_allowed_apps, "always allowed apps", "always_allowed_apps"),
            (&self.blocked_apps, "blocked apps", "blocked_apps"),
        ] {
            for (i, app) in list.iter().enumerate() {
                if app.trim().is_empty() {
                    add(format!("{field}[{i}]"), format!("One of the {label} has an empty name."));
                } else if list[..i].contains(app) {
                    add(format!("{field}[{i}]"), format!("{app} is listed twice in {label}."));
                }
            }
        }
        for (i, app) in self.blocked_apps.iter().enumerate() {
            if !app.trim().is_empty() && self.is_always_allowed(app) {
                add(
                    format!("blocked_apps[{i}]"),
                    format!("{app} is both always allowed and blocked. Choose one."),
                );
            }
        }
        if self.away_share_percent > 100 {
            add(
                "away_share_percent".into(),
                format!(
                    "The away share is {} percent, but it must be from 0 to 100.",
                    self.away_share_percent
                ),
            );
        }
        if self.lease_minutes == 0 || self.lease_minutes > MAX_LEASE_MINUTES {
            add(
                "lease_minutes".into(),
                format!(
                    "The lease is {} minutes, but it must be from 1 to {MAX_LEASE_MINUTES} minutes.",
                    self.lease_minutes
                ),
            );
        }
        for (i, m) in self.warning_minutes.iter().enumerate() {
            let field = format!("warning_minutes[{i}]");
            if *m == 0 {
                add(field, "A warning 0 minutes before a lock would come with the lock itself. Use 1 minute or more.".into());
            } else if *m > MINUTES_PER_DAY as u32 {
                add(field, format!("A warning {m} minutes before a lock is more than a day ahead."));
            } else if self.warning_minutes[..i].contains(m) {
                add(field, format!("The warning {m} minutes before a lock is listed twice."));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let rules = RuleSet::default();
        assert!(rules.validate().is_empty(), "{:?}", rules.validate());
        assert_eq!(rules.away_share_percent, 50);
        assert_eq!(rules.lease_minutes, 10);
        assert_eq!(rules.warning_minutes, vec![15, 5, 1]);
        assert_eq!(rules.day_start_minute, 0);
        assert!(RuleSet::new(3, 19_800).is_valid());
    }

    #[test]
    fn validation_returns_every_problem() {
        let mut rules = RuleSet::new(0, 15 * 3_600);
        rules.day_start_minute = 1_440;
        rules.budgets.monday = DailyBudget::Minutes(1_500);
        rules.blocked_hours = vec![
            BlockedWindow { days: vec![], start_minute: 1_260, end_minute: 1_260 },
            BlockedWindow {
                days: vec![Weekday::Friday, Weekday::Friday],
                start_minute: 1_500,
                end_minute: 1_441,
            },
        ];
        rules.always_allowed_apps = vec!["clock".into(), "".into(), "clock".into(), "game".into()];
        rules.blocked_apps = vec!["game".into()];
        rules.away_share_percent = 120;
        rules.lease_minutes = 0;
        rules.warning_minutes = vec![15, 0, 15, 2_000];
        let problems = rules.validate();
        let fields: Vec<&str> = problems.iter().map(|p| p.field.as_str()).collect();
        for expected in [
            "version",
            "utc_offset_secs",
            "day_start_minute",
            "budgets.monday",
            "blocked_hours[0].days",
            "blocked_hours[0].end_minute",
            "blocked_hours[1].days",
            "blocked_hours[1].start_minute",
            "blocked_hours[1].end_minute",
            "always_allowed_apps[1]",
            "always_allowed_apps[2]",
            "blocked_apps[0]",
            "away_share_percent",
            "lease_minutes",
            "warning_minutes[1]",
            "warning_minutes[2]",
            "warning_minutes[3]",
        ] {
            assert!(fields.contains(&expected), "missing {expected} in {problems:#?}");
        }
        assert_eq!(problems.len(), 17, "{problems:#?}");
        // Plain words, and no dash punctuation.
        for p in &problems {
            assert!(p.message.ends_with('.'), "{}", p.message);
            assert!(!p.message.contains(" - ") && !p.message.contains('\u{2014}'), "{}", p.message);
        }
    }

    #[test]
    fn odd_offsets_are_refused() {
        let mut rules = RuleSet::new(1, 19_830);
        assert_eq!(rules.validate()[0].field, "utc_offset_secs");
        rules.utc_offset_secs = 20_700; // Nepal, +05:45, is fine
        assert!(rules.is_valid());
        rules.utc_offset_secs = -12 * 3_600;
        assert!(rules.is_valid());
    }

    #[test]
    fn window_lengths() {
        let w = |s, e| BlockedWindow { days: vec![Weekday::Monday], start_minute: s, end_minute: e };
        assert_eq!(w(1_260, 420).length_minutes(), 600);
        assert!(w(1_260, 420).crosses_midnight());
        assert_eq!(w(1_260, 1_440).length_minutes(), 180);
        assert!(!w(1_260, 1_440).crosses_midnight());
        assert_eq!(w(1_260, 0).length_minutes(), 180);
        assert_eq!(w(0, 1_440).length_minutes(), 1_440);
        assert_eq!(w(600, 660).length_minutes(), 60);
        let rules = RuleSet { blocked_hours: vec![w(0, 1_440), w(1_260, 0)], ..RuleSet::default() };
        assert!(rules.is_valid());
    }

    #[test]
    fn budgets_and_away_caps() {
        let mut rules = RuleSet::new(1, 0);
        rules.budgets.sunday = DailyBudget::NoLimit;
        rules.budgets.monday = DailyBudget::Minutes(90);
        let sunday = 20_779; // 2026-11-22
        let monday = sunday + 1;
        assert_eq!(rules.budget_secs_on(sunday), None);
        assert_eq!(rules.budget_secs_on(monday), Some(5_400));
        assert_eq!(rules.away_cap_secs(50, monday), 2_700);
        assert_eq!(rules.away_cap_secs(33, monday), 1_782);
        assert_eq!(rules.away_cap_secs(0, monday), 0);
        assert_eq!(rules.away_cap_secs(50, sunday), 0);
        assert_eq!(clock_text(1_260), "21:00");
        assert_eq!(clock_text(1_440), "24:00");
    }

    #[test]
    fn warning_points_are_sorted_without_repeats() {
        let rules = RuleSet { warning_minutes: vec![1, 15, 5, 15, 0], ..RuleSet::default() };
        assert_eq!(rules.warning_points(), vec![15, 5, 1]);
    }
}
