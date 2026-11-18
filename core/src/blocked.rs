//! Blocked hours in real time: is an instant blocked, when do the blocked
//! hours end, and when do the next ones start.

use serde::{Deserialize, Serialize};

use crate::rules::RuleSet;
use crate::time::{HomeCalendar, SECONDS_PER_DAY, Weekday};

/// How far ahead blocked hours are looked at: eight days, so that a whole
/// week of windows is always seen.
pub const LOOK_AHEAD_SECS: i64 = 8 * SECONDS_PER_DAY;

/// Where an instant stands against the blocked hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedHours {
    /// Not in blocked hours.
    Open {
        /// When the next blocked hours start, if any start in the next eight
        /// days.
        next_start: Option<i64>,
    },
    /// In blocked hours.
    Blocked {
        /// When they end. Windows that touch or overlap count as one. None
        /// when they do not end within the next eight days.
        until: Option<i64>,
    },
}

impl RuleSet {
    /// Calls `each` with the start and end of every window that starts on
    /// home `date`.
    fn windows_on(&self, cal: &HomeCalendar, date: i64, mut each: impl FnMut(i64, i64)) {
        let weekday = Weekday::of_date(date);
        let midnight = cal.midnight(date);
        for window in &self.blocked_hours {
            if window.days.contains(&weekday) {
                let start = midnight.saturating_add(window.start_minute as i64 * 60);
                let end = start.saturating_add(window.length_minutes() as i64 * 60);
                each(start, end);
            }
        }
    }

    /// The latest end among the windows that cover `t`, or None.
    fn covering_end(&self, cal: &HomeCalendar, t: i64) -> Option<i64> {
        // A window lasts at most a day, so only windows starting on the date
        // of `t` or the date before can cover it.
        let date = cal.date_of(t);
        let mut latest: Option<i64> = None;
        for d in [date - 1, date] {
            self.windows_on(cal, d, |start, end| {
                if start <= t && t < end {
                    latest = Some(latest.map_or(end, |l| l.max(end)));
                }
            });
        }
        latest
    }

    /// Where `t` stands against the blocked hours.
    pub fn blocked_hours_at(&self, t: i64) -> BlockedHours {
        let cal = self.calendar();
        match self.covering_end(&cal, t) {
            None => BlockedHours::Open { next_start: self.next_blocked_start_after(&cal, t) },
            Some(mut end) => {
                let horizon = t.saturating_add(LOOK_AHEAD_SECS);
                while let Some(later) = self.covering_end(&cal, end) {
                    end = later;
                    if end > horizon {
                        return BlockedHours::Blocked { until: None };
                    }
                }
                BlockedHours::Blocked { until: Some(end) }
            }
        }
    }

    /// True when `t` falls in blocked hours.
    pub fn is_blocked_at(&self, t: i64) -> bool {
        matches!(self.blocked_hours_at(t), BlockedHours::Blocked { .. })
    }

    /// The first window start after `t` within the next eight days.
    fn next_blocked_start_after(&self, cal: &HomeCalendar, t: i64) -> Option<i64> {
        let date = cal.date_of(t);
        for d in date..=date + 8 {
            let mut first: Option<i64> = None;
            self.windows_on(cal, d, |start, _| {
                if start > t {
                    first = Some(first.map_or(start, |f| f.min(start)));
                }
            });
            // Windows on a later date all start later than any on this date.
            if first.is_some() {
                return first;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::BlockedWindow;
    use crate::time::CivilDate;

    const INDIA: i32 = 19_800;

    fn rules(windows: Vec<BlockedWindow>) -> RuleSet {
        let mut r = RuleSet::new(1, INDIA);
        r.blocked_hours = windows;
        r
    }

    fn at(day: u8, hour: u8, minute: u8) -> i64 {
        // November 2026: the 16th is a Monday.
        RuleSet::new(1, INDIA).calendar().at(CivilDate::new(2026, 11, day).unwrap(), hour, minute, 0)
    }

    fn bedtime(days: Vec<Weekday>) -> BlockedWindow {
        BlockedWindow { days, start_minute: 21 * 60, end_minute: 7 * 60 }
    }

    #[test]
    fn a_window_across_midnight_seen_from_both_sides() {
        let r = rules(vec![bedtime(vec![Weekday::Monday])]);
        assert_eq!(r.blocked_hours_at(at(16, 23, 30)), BlockedHours::Blocked { until: Some(at(17, 7, 0)) });
        assert_eq!(r.blocked_hours_at(at(17, 6, 30)), BlockedHours::Blocked { until: Some(at(17, 7, 0)) });
        assert_eq!(r.blocked_hours_at(at(16, 21, 0)), BlockedHours::Blocked { until: Some(at(17, 7, 0)) });
        // The end is not included, and Tuesday has no bedtime of its own.
        assert!(!r.is_blocked_at(at(17, 7, 0)));
        assert!(!r.is_blocked_at(at(17, 23, 0)));
        // Sunday has no bedtime, so Monday morning is open.
        assert!(!r.is_blocked_at(at(16, 6, 30)));
        assert_eq!(
            r.blocked_hours_at(at(16, 6, 30)),
            BlockedHours::Open { next_start: Some(at(16, 21, 0)) }
        );
    }

    #[test]
    fn touching_windows_count_as_one() {
        let r = rules(vec![
            BlockedWindow { days: vec![Weekday::Wednesday], start_minute: 13 * 60, end_minute: 14 * 60 },
            BlockedWindow { days: vec![Weekday::Wednesday], start_minute: 14 * 60, end_minute: 15 * 60 + 30 },
            BlockedWindow { days: vec![Weekday::Wednesday], start_minute: 15 * 60, end_minute: 15 * 60 + 10 },
        ]);
        assert_eq!(r.blocked_hours_at(at(18, 13, 30)), BlockedHours::Blocked { until: Some(at(18, 15, 30)) });
    }

    #[test]
    fn bedtime_every_night_joined_with_a_school_morning() {
        let r = rules(vec![
            bedtime(Weekday::ALL.to_vec()),
            BlockedWindow { days: vec![Weekday::Thursday], start_minute: 7 * 60, end_minute: 8 * 60 },
        ]);
        assert_eq!(r.blocked_hours_at(at(18, 22, 0)), BlockedHours::Blocked { until: Some(at(19, 8, 0)) });
    }

    #[test]
    fn blocked_all_week_has_no_end_in_sight() {
        let r = rules(vec![BlockedWindow { days: Weekday::ALL.to_vec(), start_minute: 0, end_minute: 1_440 }]);
        assert_eq!(r.blocked_hours_at(at(18, 12, 0)), BlockedHours::Blocked { until: None });
    }

    #[test]
    fn next_start_looks_a_week_ahead() {
        let r = rules(vec![BlockedWindow { days: vec![Weekday::Monday], start_minute: 600, end_minute: 660 }]);
        // From Tuesday the 17th the next one is Monday the 23rd.
        assert_eq!(r.blocked_hours_at(at(17, 12, 0)), BlockedHours::Open { next_start: Some(at(23, 10, 0)) });
        assert_eq!(rules(vec![]).blocked_hours_at(at(17, 12, 0)), BlockedHours::Open { next_start: None });
    }
}
