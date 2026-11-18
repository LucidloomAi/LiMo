//! Time: unix seconds, the home time zone, and home days.
//!
//! LiMo counts time in whole seconds since 1 January 1970 UTC ("unix
//! seconds"), always as an `i64`. Days and weekdays are always worked out in
//! the home time zone, the zone of the master PC. That zone travels with the
//! rules as a fixed offset from UTC in seconds, so a phone that is abroad
//! still counts home days, never the days of the place it is in.
//!
//! Two kinds of day numbers appear, both counted in days from 1 January 1970
//! (number 0, a Thursday):
//!
//! - a **date** is a calendar date on the wall clock at home, midnight to
//!   midnight;
//! - a **home day** is the stretch of time one daily budget covers. It starts
//!   at the rules' day start minute (midnight unless the guardian changed it)
//!   and is numbered by the date it starts on. With a day start of 04:00,
//!   home day 20775 (Wednesday 18 November 2026) runs from 04:00 on that
//!   Wednesday to 04:00 on the Thursday.
//!
//! Because a home day carries the number of the date it starts on, its
//! weekday is the weekday of that date.

use serde::{Deserialize, Serialize};

/// Seconds in one day.
pub const SECONDS_PER_DAY: i64 = 86_400;
/// Minutes in one day.
pub const MINUTES_PER_DAY: u16 = 1_440;

/// A day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday.
    Sunday,
}

impl Weekday {
    /// All seven days, Monday first.
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    /// The weekday of a date number (days since 1 January 1970, a Thursday).
    /// Works for dates before 1970 too.
    pub fn of_date(date: i64) -> Weekday {
        // Date 0 is a Thursday, which is index 3 counting Monday as 0.
        Weekday::ALL[(date.rem_euclid(7) as usize + 3) % 7]
    }

    /// Monday is 0, Sunday is 6.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// The English name, for messages: "Monday".
    pub fn name(self) -> &'static str {
        match self {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        }
    }

    /// Reads a weekday from its English name or its first three letters, in
    /// any case: "Monday", "mon", "MON".
    pub fn from_name(text: &str) -> Option<Weekday> {
        let lower = text.trim().to_ascii_lowercase();
        Weekday::ALL.into_iter().find(|day| {
            let name = day.name().to_ascii_lowercase();
            lower == name || lower == name[..3]
        })
    }
}

/// A calendar date in the proleptic Gregorian calendar (the calendar in use
/// today, carried back and forward without end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CivilDate {
    /// The year. Year 0 is 1 BC, year -1 is 2 BC.
    pub year: i64,
    /// The month, 1 to 12.
    pub month: u8,
    /// The day of the month, 1 to 31.
    pub day: u8,
}

impl CivilDate {
    /// A date, or None when there is no such date (30 February, month 13).
    pub fn new(year: i64, month: u8, day: u8) -> Option<CivilDate> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(CivilDate { year, month, day })
    }

    /// The date number of this date: days since 1 January 1970.
    pub fn date_number(&self) -> i64 {
        // Count years from March, so that the leap day is the last day of the
        // counted year, then count whole 400 year cycles of 146097 days.
        let month = self.month as i64;
        let year = if month <= 2 { self.year - 1 } else { self.year };
        let cycle = year.div_euclid(400);
        let year_of_cycle = year - cycle * 400;
        let month_from_march = (month + 9) % 12;
        let day_of_year = (153 * month_from_march + 2) / 5 + self.day as i64 - 1;
        let day_of_cycle =
            year_of_cycle * 365 + year_of_cycle / 4 - year_of_cycle / 100 + day_of_year;
        // 719468 is the number of days from 1 March of year 0 to 1 January 1970.
        cycle * 146_097 + day_of_cycle - 719_468
    }

    /// The date with the given date number.
    pub fn from_date_number(date: i64) -> CivilDate {
        let shifted = date + 719_468;
        let cycle = shifted.div_euclid(146_097);
        let day_of_cycle = shifted - cycle * 146_097;
        let year_of_cycle = (day_of_cycle - day_of_cycle / 1_460 + day_of_cycle / 36_524
            - day_of_cycle / 146_096)
            / 365;
        let day_of_year =
            day_of_cycle - (365 * year_of_cycle + year_of_cycle / 4 - year_of_cycle / 100);
        let month_from_march = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
        let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
        let year = year_of_cycle + cycle * 400 + if month <= 2 { 1 } else { 0 };
        CivilDate { year, month: month as u8, day: day as u8 }
    }

    /// The weekday of this date.
    pub fn weekday(&self) -> Weekday {
        Weekday::of_date(self.date_number())
    }
}

fn is_leap_year(year: i64) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A moment as the wall clock at home shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTime {
    /// The date at home.
    pub date: CivilDate,
    /// The weekday of that date.
    pub weekday: Weekday,
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Second, 0 to 59.
    pub second: u8,
}

/// One part of a stretch of time that lies inside a single home day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaySpan {
    /// The home day this part belongs to.
    pub day: i64,
    /// Start, unix seconds, included.
    pub start: i64,
    /// End, unix seconds, not included.
    pub end: i64,
}

/// The home calendar: the home time zone and the minute a new day starts.
/// Built from the rules with [`crate::rules::RuleSet::calendar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeCalendar {
    /// Home time minus UTC, in seconds: 19800 for India (+05:30), -18000 for
    /// New York in winter (-05:00).
    pub utc_offset_secs: i32,
    /// The minute of the day at which a new home day starts, 0 to 1439.
    /// 0 is midnight.
    pub day_start_minute: u16,
}

impl HomeCalendar {
    /// A calendar with this offset and day start.
    pub fn new(utc_offset_secs: i32, day_start_minute: u16) -> HomeCalendar {
        HomeCalendar { utc_offset_secs, day_start_minute }
    }

    /// Seconds to add to unix seconds to reach the start of the counted day.
    fn shift(&self, day_start_minute: u16) -> i64 {
        self.utc_offset_secs as i64 - day_start_minute as i64 * 60
    }

    /// Day number of `t` once `shift` is added, without overflow for any `t`.
    fn day_number(t: i64, shift: i64) -> i64 {
        let base = t.div_euclid(SECONDS_PER_DAY);
        let rest = t.rem_euclid(SECONDS_PER_DAY) + shift;
        base + rest.div_euclid(SECONDS_PER_DAY)
    }

    /// The date on the wall clock at home at `t`.
    pub fn date_of(&self, t: i64) -> i64 {
        Self::day_number(t, self.shift(0))
    }

    /// Seconds since the last midnight at home, 0 to 86399.
    pub fn second_of_date(&self, t: i64) -> i64 {
        (t.rem_euclid(SECONDS_PER_DAY) + self.shift(0)).rem_euclid(SECONDS_PER_DAY)
    }

    /// The minute of the day at home, 0 to 1439 (21:00 is 1260).
    pub fn minute_of_date(&self, t: i64) -> u16 {
        (self.second_of_date(t) / 60) as u16
    }

    /// The unix second of midnight at home at the start of `date`.
    pub fn midnight(&self, date: i64) -> i64 {
        date.saturating_mul(SECONDS_PER_DAY).saturating_sub(self.utc_offset_secs as i64)
    }

    /// The home day that `t` belongs to.
    pub fn day_of(&self, t: i64) -> i64 {
        Self::day_number(t, self.shift(self.day_start_minute))
    }

    /// The unix second at which home day `day` starts.
    pub fn day_start(&self, day: i64) -> i64 {
        self.midnight(day).saturating_add(self.day_start_minute as i64 * 60)
    }

    /// The weekday of home day `day`: the weekday of the date it starts on.
    pub fn weekday_of_day(&self, day: i64) -> Weekday {
        Weekday::of_date(day)
    }

    /// Splits the stretch from `start` to `end` at every home day boundary.
    /// Returns nothing when `end` is not after `start`.
    pub fn split(&self, start: i64, end: i64) -> Vec<DaySpan> {
        let mut parts = Vec::new();
        let mut from = start;
        while from < end {
            let day = self.day_of(from);
            let next_start = self.day_start(day.saturating_add(1));
            let to = if next_start > from { next_start.min(end) } else { end };
            parts.push(DaySpan { day, start: from, end: to });
            from = to;
        }
        parts
    }

    /// The unix second at which the wall clock at home shows this date and
    /// time.
    pub fn at(&self, date: CivilDate, hour: u8, minute: u8, second: u8) -> i64 {
        self.midnight(date.date_number())
            .saturating_add(hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
    }

    /// What the wall clock at home shows at `t`.
    pub fn local(&self, t: i64) -> LocalTime {
        let date = CivilDate::from_date_number(self.date_of(t));
        let second = self.second_of_date(t);
        LocalTime {
            date,
            weekday: date.weekday(),
            hour: (second / 3_600) as u8,
            minute: (second / 60 % 60) as u8,
            second: (second % 60) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDIA: i32 = 19_800;
    const NEW_YORK: i32 = -18_000;

    fn date(y: i64, m: u8, d: u8) -> CivilDate {
        CivilDate::new(y, m, d).unwrap()
    }

    #[test]
    fn known_date_numbers() {
        assert_eq!(date(1970, 1, 1).date_number(), 0);
        assert_eq!(date(1969, 12, 31).date_number(), -1);
        assert_eq!(date(2000, 1, 1).date_number(), 10_957);
        assert_eq!(date(2000, 3, 1).date_number(), 11_017);
        assert_eq!(date(1, 1, 1).date_number(), -719_162);
        assert_eq!(date(9999, 12, 31).date_number(), 2_932_896);
    }

    #[test]
    fn date_numbers_go_both_ways_over_a_wide_range() {
        // Every 97th day across more than 1100 years, both sides of 1970,
        // including leap days and century years.
        let mut n = -400_000;
        while n < 400_000 {
            let d = CivilDate::from_date_number(n);
            assert_eq!(d.date_number(), n, "{d:?}");
            assert!(CivilDate::new(d.year, d.month, d.day).is_some());
            n += 97;
        }
    }

    #[test]
    fn far_past_and_far_future_dates() {
        for n in [-1_000_000_000_i64, -3_000_000, 3_000_000, 1_000_000_000, 100_000_000_000_000] {
            assert_eq!(CivilDate::from_date_number(n).date_number(), n);
        }
        // Unix seconds a thousand years ahead and behind.
        let cal = HomeCalendar::new(INDIA, 0);
        let far = 32_503_680_000; // 3000-01-01 00:00 UTC
        assert_eq!(cal.local(far).date, date(3000, 1, 1));
        assert_eq!(cal.local(far).hour, 5);
        assert_eq!(cal.local(far).minute, 30);
        let long_ago = -30_610_224_000; // 1000-01-01 00:00 UTC
        assert_eq!(cal.local(long_ago).date, date(1000, 1, 1));
        assert_eq!(cal.date_of(long_ago), date(1000, 1, 1).date_number());
    }

    #[test]
    fn extreme_unix_seconds_do_not_overflow() {
        let cal = HomeCalendar::new(-50_400, 1_439);
        for t in [i64::MIN, i64::MIN + 1, i64::MAX - 1, i64::MAX] {
            let day = cal.day_of(t);
            let _ = cal.day_start(day);
            let _ = cal.date_of(t);
            let _ = cal.minute_of_date(t);
            let _ = Weekday::of_date(day);
        }
    }

    #[test]
    fn leap_years() {
        assert!(CivilDate::new(2024, 2, 29).is_some());
        assert!(CivilDate::new(2026, 2, 29).is_none());
        assert!(CivilDate::new(1900, 2, 29).is_none());
        assert!(CivilDate::new(2000, 2, 29).is_some());
        assert!(CivilDate::new(2026, 13, 1).is_none());
        assert!(CivilDate::new(2026, 4, 31).is_none());
    }

    #[test]
    fn weekdays() {
        assert_eq!(Weekday::of_date(0), Weekday::Thursday);
        assert_eq!(Weekday::of_date(-1), Weekday::Wednesday);
        assert_eq!(Weekday::of_date(-4), Weekday::Sunday);
        assert_eq!(date(2026, 11, 18).weekday(), Weekday::Wednesday);
        assert_eq!(date(2026, 11, 22).weekday(), Weekday::Sunday);
        assert_eq!(date(1, 1, 1).weekday(), Weekday::Monday);
        assert_eq!(Weekday::from_name("wed"), Some(Weekday::Wednesday));
        assert_eq!(Weekday::from_name(" Sunday "), Some(Weekday::Sunday));
        assert_eq!(Weekday::from_name("we"), None);
    }

    #[test]
    fn day_and_weekday_from_unix_seconds_and_offset() {
        // 2026-11-18 18:30 UTC is 2026-11-19 00:00 in India and 13:30 in New York.
        let t = 1_795_026_600;
        let india = HomeCalendar::new(INDIA, 0);
        let new_york = HomeCalendar::new(NEW_YORK, 0);
        assert_eq!(india.local(t).date, date(2026, 11, 19));
        assert_eq!(india.local(t).hour, 0);
        assert_eq!(india.weekday_of_day(india.day_of(t)), Weekday::Thursday);
        assert_eq!(new_york.local(t).date, date(2026, 11, 18));
        assert_eq!(new_york.local(t).hour, 13);
        assert_eq!(new_york.weekday_of_day(new_york.day_of(t)), Weekday::Wednesday);
        // One second earlier it is still Wednesday in India.
        assert_eq!(india.weekday_of_day(india.day_of(t - 1)), Weekday::Wednesday);
    }

    #[test]
    fn negative_offsets_before_1970() {
        let cal = HomeCalendar::new(NEW_YORK, 0);
        // 1969-12-31 19:00 in New York is 1970-01-01 00:00 UTC.
        assert_eq!(cal.date_of(0), -1);
        assert_eq!(cal.local(0).hour, 19);
        assert_eq!(cal.day_of(-1), -1);
        assert_eq!(cal.day_start(-1), 18_000 - SECONDS_PER_DAY);
        assert_eq!(cal.at(date(1969, 12, 31), 19, 0, 0), 0);
    }

    #[test]
    fn day_start_not_at_midnight() {
        let cal = HomeCalendar::new(INDIA, 240); // the day starts at 04:00
        let wed = date(2026, 11, 18);
        let thu = date(2026, 11, 19);
        let wed_day = wed.date_number();
        // 03:59 on Thursday still belongs to Wednesday's home day.
        assert_eq!(cal.day_of(cal.at(thu, 3, 59, 59)), wed_day);
        assert_eq!(cal.day_of(cal.at(thu, 4, 0, 0)), wed_day + 1);
        assert_eq!(cal.day_of(cal.at(wed, 4, 0, 0)), wed_day);
        assert_eq!(cal.day_of(cal.at(wed, 3, 0, 0)), wed_day - 1);
        assert_eq!(cal.day_start(wed_day), cal.at(wed, 4, 0, 0));
        assert_eq!(cal.weekday_of_day(cal.day_of(cal.at(thu, 2, 0, 0))), Weekday::Wednesday);
    }

    #[test]
    fn split_at_midnight_and_at_a_late_day_start() {
        let cal = HomeCalendar::new(INDIA, 0);
        let start = cal.at(date(2026, 11, 18), 23, 50, 0);
        let end = cal.at(date(2026, 11, 19), 0, 20, 0);
        let parts = cal.split(start, end);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].end, cal.at(date(2026, 11, 19), 0, 0, 0));
        assert_eq!(parts[0].end - parts[0].start, 600);
        assert_eq!(parts[1].end - parts[1].start, 1_200);
        assert_eq!(parts[1].day, parts[0].day + 1);

        let late = HomeCalendar::new(INDIA, 240);
        assert_eq!(late.split(start, end).len(), 1);
        let three_days = late.split(start, start + 3 * SECONDS_PER_DAY);
        assert_eq!(three_days.len(), 4);
        let total: i64 = three_days.iter().map(|p| p.end - p.start).sum();
        assert_eq!(total, 3 * SECONDS_PER_DAY);
        assert!(cal.split(end, start).is_empty());
        assert!(cal.split(start, start).is_empty());
    }
}
