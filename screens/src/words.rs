//! Times and spans of time as words a child can read: "Tomorrow, 7:00 am",
//! "Today, 9:30 pm", "1 h 12 min".
//!
//! Every function here is pure. A moment is unix seconds; the PC's clock is
//! its offset from UTC in minutes and whether it shows 12 or 24 hours.

/// How the PC shows the time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Clock {
    /// "9:42 pm", "7:00 am", "12:00 am" for midnight.
    #[default]
    Hours12,
    /// "21:42", "07:00", "00:00" for midnight.
    Hours24,
}

/// The PC's clock: its offset from UTC and how it shows the time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PcClock {
    /// Minutes east of UTC: 330 for India, -300 for New York in winter.
    /// Kept between -18 and +18 hours.
    pub utc_offset_minutes: i32,
    /// 12 or 24 hours.
    pub clock: Clock,
}

impl PcClock {
    /// A clock at this offset.
    pub const fn new(utc_offset_minutes: i32, clock: Clock) -> PcClock {
        PcClock { utc_offset_minutes, clock }
    }

    fn offset_secs(self) -> i64 {
        i64::from(self.utc_offset_minutes.clamp(-18 * 60, 18 * 60)) * 60
    }
}

/// The earliest moment shown: 1 January of the year 1.
const EARLIEST: i64 = -62_135_596_800;
/// The latest moment shown: the end of the year 9999.
const LATEST: i64 = 253_402_300_799;

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// A moment on the PC's own clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    /// Days since 1 January 1970 on the PC's clock.
    pub day: i64,
    /// The year.
    pub year: i64,
    /// The month, 1 to 12.
    pub month: u32,
    /// The day of the month, 1 to 31.
    pub date: u32,
    /// The day of the week, 0 for Monday to 6 for Sunday.
    pub weekday: u32,
    /// The hour, 0 to 23.
    pub hour: u32,
    /// The minute, 0 to 59.
    pub minute: u32,
    /// The second, 0 to 59.
    pub second: u32,
}

/// A moment, on the PC's clock.
pub fn local(at: i64, pc: PcClock) -> Local {
    let at = at.clamp(EARLIEST, LATEST) + pc.offset_secs();
    let day = at.div_euclid(86_400);
    let secs = at.rem_euclid(86_400);
    let (year, month, date) = civil_from_days(day);
    Local {
        day,
        year,
        month,
        date,
        weekday: (day + 3).rem_euclid(7) as u32,
        hour: (secs / 3600) as u32,
        minute: ((secs / 60) % 60) as u32,
        second: (secs % 60) as u32,
    }
}

/// The year, month and day of a count of days since 1 January 1970, by the
/// proleptic Gregorian calendar (Howard Hinnant's method).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The time of day of a moment, as the PC shows it: "9:42 pm" or "21:42".
/// The seconds are dropped.
pub fn time_of_day(at: i64, pc: PcClock) -> String {
    hour_minute(local(at, pc), pc.clock)
}

fn hour_minute(t: Local, clock: Clock) -> String {
    match clock {
        Clock::Hours24 => format!("{:02}:{:02}", t.hour, t.minute),
        Clock::Hours12 => {
            let half = if t.hour < 12 { "am" } else { "pm" };
            let hour = match t.hour % 12 {
                0 => 12,
                h => h,
            };
            format!("{hour}:{:02} {half}", t.minute)
        }
    }
}

/// A moment rounded up to the next whole minute, so that a time something
/// opens is never said earlier than it is.
pub fn up_to_minute(at: i64) -> i64 {
    let at = at.clamp(EARLIEST, LATEST);
    let over = at.rem_euclid(60);
    if over == 0 { at } else { at + (60 - over) }
}

/// The date at the top of the lock screen: "Wednesday 18 November".
pub fn date_line(at: i64, pc: PcClock) -> String {
    let t = local(at, pc);
    format!("{} {} {}", weekday_name(t), t.date, month_name(t))
}

fn weekday_name(t: Local) -> &'static str {
    WEEKDAYS.get(t.weekday as usize).copied().unwrap_or("")
}

fn month_name(t: Local) -> &'static str {
    MONTHS.get(t.month.saturating_sub(1) as usize).copied().unwrap_or("")
}

/// Which day a later moment is on, said from now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Day {
    /// It has come already, or comes within the minute.
    Now,
    /// Today.
    Today,
    /// Tomorrow.
    Tomorrow,
    /// Within the week: the name of the day, such as "Friday".
    Weekday(&'static str),
    /// Further on: such as "Friday 27 November".
    Date(String),
}

/// Which day `at` falls on, from `now`.
pub fn day_of(now: i64, at: i64, pc: PcClock) -> Day {
    if at <= now {
        return Day::Now;
    }
    let today = local(now, pc).day;
    let then = local(at, pc);
    match then.day - today {
        d if d <= 0 => Day::Today,
        1 => Day::Tomorrow,
        2..=6 => Day::Weekday(weekday_name(then)),
        _ => Day::Date(format!("{} {} {}", weekday_name(then), then.date, month_name(then))),
    }
}

/// When something opens again, for the lock screen: "Today, 9:30 pm",
/// "Tomorrow, 7:00 am", "Friday, 7:00 am", "Friday 27 November, 7:00 am",
/// or "In a moment" when it is now or past. Rounded up to the minute.
pub fn opens_again(now: i64, at: i64, pc: PcClock) -> String {
    let at = up_to_minute(at);
    let time = time_of_day(at, pc);
    match day_of(now, at, pc) {
        Day::Now => "In a moment".to_string(),
        Day::Today => format!("Today, {time}"),
        Day::Tomorrow => format!("Tomorrow, {time}"),
        Day::Weekday(name) => format!("{name}, {time}"),
        Day::Date(date) => format!("{date}, {time}"),
    }
}

/// The same moment inside a sentence: "9:47 pm" today, else "tomorrow,
/// 7:00 am", "Friday, 7:00 am" and so on. Rounded up to the minute.
pub fn until_in_sentence(now: i64, at: i64, pc: PcClock) -> String {
    let at = up_to_minute(at);
    let time = time_of_day(at, pc);
    match day_of(now, at, pc) {
        Day::Now | Day::Today => time,
        Day::Tomorrow => format!("tomorrow, {time}"),
        Day::Weekday(name) => format!("{name}, {time}"),
        Day::Date(date) => format!("{date}, {time}"),
    }
}

/// The end of the PC's day that `now` is in, as unix seconds: the next
/// midnight on the PC's clock.
pub fn end_of_day(now: i64, pc: PcClock) -> i64 {
    let day = local(now, pc).day;
    (day + 1) * 86_400 - pc.offset_secs()
}

/// A span of minutes in a sentence: "30 min", "1 hour", "2 hours",
/// "1 hour 30 min".
pub fn minutes(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (1, 0) => "1 hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (1, m) => format!("1 hour {m} min"),
        (h, m) => format!("{h} hours {m} min"),
    }
}

/// A span of minutes in full: "30 minutes", "1 minute", "1 hour",
/// "2 hours", "1 hour 30 minutes".
pub fn long_minutes(m: u32) -> String {
    let mins = |m: u32| if m == 1 { "1 minute".to_string() } else { format!("{m} minutes") };
    match (m / 60, m % 60) {
        (0, m) => mins(m),
        (1, 0) => "1 hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (1, m) => format!("1 hour {}", mins(m)),
        (h, m) => format!("{h} hours {}", mins(m)),
    }
}

/// The time left on the time left display: "1 h 12 min", "2 h", "14 min",
/// or minutes and seconds under a minute, "0:42". Minutes are rounded up,
/// so the display never says less is left than is.
pub fn time_left(seconds: u64) -> String {
    if seconds < 60 {
        return format!("0:{seconds:02}");
    }
    let m = seconds.div_ceil(60);
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// A countdown of under an hour: "0:42", "4:05". Longer ones say minutes.
pub fn countdown(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 {
        return time_left(s as u64);
    }
    format!("{}:{:02}", s / 60, s % 60)
}

/// An ASCII apostrophe between letters as the typographic one, as the
/// design pages show the words: "Today’s".
pub fn typographic(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, c) in chars.iter().enumerate() {
        let before = i.checked_sub(1).and_then(|j| chars.get(j)).is_some_and(|c| c.is_alphanumeric());
        let after = chars.get(i + 1).is_some_and(|c| c.is_alphanumeric());
        if *c == '\'' && before && after {
            out.push('\u{2019}');
        } else {
            out.push(*c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const H12: PcClock = PcClock::new(0, Clock::Hours12);
    const H24: PcClock = PcClock::new(0, Clock::Hours24);
    /// Wednesday 18 November 2026, 00:00 UTC.
    const WED: i64 = 1_794_960_000;

    fn at(day: i64, hour: i64, minute: i64) -> i64 {
        WED + day * 86_400 + hour * 3600 + minute * 60
    }

    #[test]
    fn a_known_day_is_read_right() {
        let t = local(WED, H12);
        assert_eq!((t.year, t.month, t.date, t.weekday), (2026, 11, 18, 2));
        assert_eq!(date_line(at(0, 21, 42), H12), "Wednesday 18 November");
        assert_eq!(local(0, H12).weekday, 3, "1 January 1970 was a Thursday");
        assert_eq!(date_line(951_782_400, H12), "Tuesday 29 February", "2000 was a leap year");
    }

    #[test]
    fn the_time_of_day_in_twelve_and_twenty_four_hours() {
        assert_eq!(time_of_day(at(0, 21, 42), H12), "9:42 pm");
        assert_eq!(time_of_day(at(0, 21, 42), H24), "21:42");
        assert_eq!(time_of_day(at(0, 7, 0), H12), "7:00 am");
        assert_eq!(time_of_day(at(0, 7, 0), H24), "07:00");
        assert_eq!(time_of_day(at(0, 0, 0), H12), "12:00 am", "midnight");
        assert_eq!(time_of_day(at(0, 0, 5), H24), "00:05");
        assert_eq!(time_of_day(at(0, 12, 0), H12), "12:00 pm", "noon");
        assert_eq!(time_of_day(at(0, 12, 59) + 59, H12), "12:59 pm", "seconds are dropped");
    }

    #[test]
    fn opening_times_are_said_from_now() {
        let now = at(0, 21, 42);
        assert_eq!(opens_again(now, at(0, 22, 30), H12), "Today, 10:30 pm");
        assert_eq!(opens_again(at(0, 9, 0), at(0, 21, 30), H12), "Today, 9:30 pm");
        assert_eq!(opens_again(now, at(1, 7, 0), H12), "Tomorrow, 7:00 am");
        assert_eq!(opens_again(now, at(1, 7, 0), H24), "Tomorrow, 07:00");
        assert_eq!(opens_again(now, at(2, 7, 0), H12), "Friday, 7:00 am");
        assert_eq!(opens_again(now, at(6, 7, 0), H12), "Tuesday, 7:00 am");
        assert_eq!(opens_again(now, at(9, 7, 0), H12), "Friday 27 November, 7:00 am");
        assert_eq!(opens_again(now, now, H12), "In a moment");
        assert_eq!(opens_again(now, now - 600, H12), "In a moment");
    }

    #[test]
    fn midnight_belongs_to_the_next_day() {
        let now = at(0, 21, 42);
        assert_eq!(opens_again(now, at(1, 0, 0), H12), "Tomorrow, 12:00 am");
        assert_eq!(opens_again(now, at(1, 0, 0), H24), "Tomorrow, 00:00");
        // A minute before midnight is still today.
        assert_eq!(opens_again(now, at(0, 23, 59), H12), "Today, 11:59 pm");
        // Just after midnight, a time later that morning is today.
        assert_eq!(opens_again(at(1, 0, 1), at(1, 7, 0), H12), "Today, 7:00 am");
    }

    #[test]
    fn a_time_is_rounded_up_to_the_minute() {
        let now = at(0, 21, 0);
        assert_eq!(opens_again(now, at(0, 21, 46) + 1, H12), "Today, 9:47 pm");
        assert_eq!(until_in_sentence(now, at(0, 21, 46) + 59, H12), "9:47 pm");
        // Rounding up across midnight changes the day.
        assert_eq!(opens_again(now, at(0, 23, 59) + 30, H12), "Tomorrow, 12:00 am");
        assert_eq!(up_to_minute(120), 120);
        assert_eq!(up_to_minute(121), 180);
        assert_eq!(up_to_minute(-59), 0);
    }

    #[test]
    fn the_offset_decides_the_day() {
        // 23:30 UTC is already tomorrow in India and still today in New York.
        let india = PcClock::new(330, Clock::Hours12);
        let new_york = PcClock::new(-300, Clock::Hours12);
        let utc_late = at(0, 23, 30);
        assert_eq!(date_line(utc_late, india), "Thursday 19 November");
        assert_eq!(time_of_day(utc_late, india), "5:00 am");
        assert_eq!(date_line(utc_late, new_york), "Wednesday 18 November");
        assert_eq!(opens_again(at(0, 17, 0), utc_late, india), "Tomorrow, 5:00 am");
        assert_eq!(opens_again(at(0, 20, 0), utc_late, india), "Today, 5:00 am");
        // The end of the day is the PC's midnight.
        assert_eq!(end_of_day(at(0, 21, 42), H12), at(1, 0, 0));
        assert_eq!(end_of_day(utc_late, india), at(1, 18, 30));
        assert_eq!(end_of_day(at(1, 0, 0), H12), at(2, 0, 0), "exactly midnight starts a new day");
    }

    #[test]
    fn in_a_sentence_today_has_no_day_word() {
        let now = at(0, 21, 42);
        assert_eq!(until_in_sentence(now, at(0, 21, 47), H12), "9:47 pm");
        assert_eq!(until_in_sentence(now, at(1, 7, 0), H12), "tomorrow, 7:00 am");
        assert_eq!(until_in_sentence(now, at(3, 7, 0), H24), "Saturday, 07:00");
    }

    #[test]
    fn spans_of_time() {
        assert_eq!(minutes(15), "15 min");
        assert_eq!(minutes(60), "1 hour");
        assert_eq!(minutes(120), "2 hours");
        assert_eq!(minutes(90), "1 hour 30 min");
        assert_eq!(minutes(150), "2 hours 30 min");
        assert_eq!(long_minutes(30), "30 minutes");
        assert_eq!(long_minutes(1), "1 minute");
        assert_eq!(long_minutes(60), "1 hour");
        assert_eq!(long_minutes(90), "1 hour 30 minutes");
        assert_eq!(long_minutes(121), "2 hours 1 minute");
        assert_eq!(time_left(4320), "1 h 12 min");
        assert_eq!(time_left(4261), "1 h 12 min", "minutes are rounded up");
        assert_eq!(time_left(7200), "2 h");
        assert_eq!(time_left(840), "14 min");
        assert_eq!(time_left(60), "1 min");
        assert_eq!(time_left(42), "0:42");
        assert_eq!(time_left(0), "0:00");
        assert_eq!(countdown(42), "0:42");
        assert_eq!(countdown(245), "4:05");
        assert_eq!(countdown(-5), "0:00");
    }

    #[test]
    fn extreme_moments_do_not_panic() {
        for t in [i64::MIN, i64::MAX, 0, -1, EARLIEST - 1, LATEST + 1] {
            let _ = date_line(t, PcClock::new(i32::MAX, Clock::Hours12));
            let _ = opens_again(t, i64::MAX, PcClock::new(i32::MIN, Clock::Hours24));
            let _ = until_in_sentence(i64::MIN, t, H12);
            let _ = end_of_day(t, H12);
        }
        let _ = time_left(u64::MAX);
        let _ = countdown(i64::MAX);
        let _ = minutes(u32::MAX);
    }

    #[test]
    fn apostrophes_between_letters_are_typographic() {
        assert_eq!(typographic("Today's time"), "Today\u{2019}s time");
        assert_eq!(typographic("'quoted'"), "'quoted'");
        assert_eq!(typographic("family's rules."), "family\u{2019}s rules.");
    }
}
