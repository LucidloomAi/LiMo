//! Every sentence the phone shows, in one place.
//!
//! They are the PC's own sentences (the agent's lock reasons and warnings,
//! the lock screen's answers to a code or a PIN, its wait and its ways out),
//! with "PC" changed to "phone" and "programs" to "apps", three settled by
//! the lead from the design sheet (today's time used up, the PIN's line, the
//! block screen naming the app), and a few that only a phone needs: its away
//! time used up, more coming when it is back home, and the way back to a
//! call. Apostrophes are plain ASCII ones, as the agent writes them.
//!
//! Times are said on the phone's own clock: its own zone, and 12 or 24 hours
//! as the phone shows them ("9:42 pm", "21:42").

use limo_core::{
    BlockedWindow, DailyBudget, HomeCalendar, LockReason, RequestAnswer, RequestView, RuleSet, SECONDS_PER_DAY,
    TryOutcome, WarningCause, Weekday,
};

use crate::facts::Permission;
use crate::orders::{LockWhy, Way};
use crate::today::LeftKind;

/// How the phone shows times: its zone, and 12 or 24 hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PhoneClock {
    /// Seconds from UTC, kept between -18 and +18 hours.
    pub(crate) utc_offset_secs: i32,
    /// True for "21:42", false for "9:42 pm".
    pub(crate) hours_24: bool,
}

const MAX_OFFSET_SECS: i32 = 18 * 3_600;
/// The earliest moment said: 1 January of the year 1.
const EARLIEST: i64 = -62_135_596_800;
/// The latest moment said: the end of the year 9999.
const LATEST: i64 = 253_402_300_799;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

impl PhoneClock {
    fn calendar(self) -> HomeCalendar {
        HomeCalendar::new(self.utc_offset_secs.clamp(-MAX_OFFSET_SECS, MAX_OFFSET_SECS), 0)
    }
}

/// The time of day of a moment: "9:42 pm" or "21:42". Seconds are dropped.
pub(crate) fn time_of_day(at: i64, clock: PhoneClock) -> String {
    let t = clock.calendar().local(at.clamp(EARLIEST, LATEST));
    if clock.hours_24 {
        return format!("{:02}:{:02}", t.hour, t.minute);
    }
    let half = if t.hour < 12 { "am" } else { "pm" };
    let hour = match t.hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{hour}:{:02} {half}", t.minute)
}

/// A moment rounded up to the next whole minute, so that a time something
/// opens is never said earlier than it is.
fn up_to_minute(at: i64) -> i64 {
    let at = at.clamp(EARLIEST, LATEST);
    let over = at.rem_euclid(60);
    if over == 0 { at } else { at + (60 - over) }
}

/// Which day a later moment is on, said from now.
enum Day {
    Now,
    Today,
    Tomorrow,
    Weekday(&'static str),
    Date(String),
}

fn day_of(now: i64, at: i64, clock: PhoneClock) -> Day {
    if at <= now {
        return Day::Now;
    }
    let cal = clock.calendar();
    let today = cal.date_of(now.clamp(EARLIEST, LATEST));
    let then = cal.local(at);
    match cal.date_of(at).saturating_sub(today) {
        d if d <= 0 => Day::Today,
        1 => Day::Tomorrow,
        2..=6 => Day::Weekday(then.weekday.name()),
        _ => Day::Date(format!(
            "{} {} {}",
            then.weekday.name(),
            then.date.day,
            MONTHS.get(usize::from(then.date.month).saturating_sub(1)).copied().unwrap_or("")
        )),
    }
}

/// When something opens again, for the lock: "Today, 9:30 pm", "Tomorrow,
/// 7:00 am", "Friday, 7:00 am", "Friday 27 November, 7:00 am", or "In a
/// moment" when it is now or past. Rounded up to the minute.
pub(crate) fn opens_again(now: i64, at: i64, clock: PhoneClock) -> String {
    let at = up_to_minute(at);
    let time = time_of_day(at, clock);
    match day_of(now, at, clock) {
        Day::Now => "In a moment".to_string(),
        Day::Today => format!("Today, {time}"),
        Day::Tomorrow => format!("Tomorrow, {time}"),
        Day::Weekday(name) => format!("{name}, {time}"),
        Day::Date(date) => format!("{date}, {time}"),
    }
}

/// The same moment inside a sentence: "9:47 pm" today, else "tomorrow,
/// 7:00 am", "Friday, 7:00 am" and so on. Rounded up to the minute.
pub(crate) fn until_in_sentence(now: i64, at: i64, clock: PhoneClock) -> String {
    let at = up_to_minute(at);
    let time = time_of_day(at, clock);
    match day_of(now, at, clock) {
        Day::Now | Day::Today => time,
        Day::Tomorrow => format!("tomorrow, {time}"),
        Day::Weekday(name) => format!("{name}, {time}"),
        Day::Date(date) => format!("{date}, {time}"),
    }
}

/// A span of minutes in a sentence: "30 min", "1 hour", "1 hour 30 min".
pub(crate) fn minutes(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (1, 0) => "1 hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (1, m) => format!("1 hour {m} min"),
        (h, m) => format!("{h} hours {m} min"),
    }
}

/// A span of minutes in full: "1 minute", "30 minutes", "1 hour 30
/// minutes".
fn long_minutes(m: u32) -> String {
    let mins = |m: u32| if m == 1 { "1 minute".to_string() } else { format!("{m} minutes") };
    match (m / 60, m % 60) {
        (0, m) => mins(m),
        (1, 0) => "1 hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (1, m) => format!("1 hour {}", mins(m)),
        (h, m) => format!("{h} hours {}", mins(m)),
    }
}

/// The time left as the time left display says it: "1 h 12 min", "2 h",
/// "14 min", or "0:42" under a minute. Minutes are rounded up, so it never
/// says less is left than is.
pub(crate) fn time_left(seconds: u64) -> String {
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

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ---------- The lock ----------

impl LockWhy {
    /// The lock for a reason the rules core gives. `in_reach` tells "waiting
    /// for time" (the master is being asked) from "the away time is used up"
    /// (more comes at home).
    pub(crate) fn from_core(reason: LockReason, in_reach: bool) -> LockWhy {
        match reason {
            LockReason::BlockedHours => LockWhy::BlockedHours,
            LockReason::NoTimeToday => LockWhy::NoTimeToday,
            LockReason::TimeUsedUp => LockWhy::TimeUsedUp,
            LockReason::NeedsMaster if in_reach => LockWhy::WaitingForTime,
            LockReason::NeedsMaster => LockWhy::AwayTimeUsedUp,
            LockReason::HeldByAnotherDevice => LockWhy::HeldByAnotherDevice,
            LockReason::GuardianLock => LockWhy::GuardianLock,
            LockReason::AppBlocked => LockWhy::AppBlocked,
        }
    }

    /// Why, in a few words, for the top of the lock.
    pub fn title(self) -> &'static str {
        match self {
            LockWhy::BlockedHours => "Screen time is off for now.",
            LockWhy::NoTimeToday => "No screen time today.",
            LockWhy::TimeUsedUp => "Today's time is used up.",
            LockWhy::WaitingForTime => "Getting more time.",
            LockWhy::AwayTimeUsedUp => "The away time is used up.",
            LockWhy::HeldByAnotherDevice => "The rest is on another device.",
            LockWhy::GuardianLock => "Locked by a guardian.",
            LockWhy::CannotCheck => "LiMo cannot check the time.",
            LockWhy::AppBlocked => "This app is not allowed.",
        }
    }

    /// Why, in plain words.
    pub fn message(self) -> &'static str {
        match self {
            LockWhy::BlockedHours => "This is a time when the phone is not used.",
            LockWhy::NoTimeToday => "Today has no screen time.",
            LockWhy::TimeUsedUp => "You have used all of today's screen time.",
            LockWhy::WaitingForTime => "LiMo is getting more time. This takes a moment.",
            LockWhy::AwayTimeUsedUp => {
                "This phone has used all the time it may use away from home. More comes when it is back home."
            }
            LockWhy::HeldByAnotherDevice => {
                "The rest of today's time is on another device. It comes here when that device is back home."
            }
            LockWhy::GuardianLock => "A guardian has locked the phone for now.",
            LockWhy::CannotCheck => "LiMo cannot check the time left just now. Ask a guardian.",
            LockWhy::AppBlocked => "It is blocked by your family's rules.",
        }
    }

    /// The small stamp on the lock: "Locked", or "One moment" while more
    /// time is on its way.
    pub(crate) fn stamp(self) -> &'static str {
        if self == LockWhy::WaitingForTime { "One moment" } else { "Locked" }
    }

    /// When it opens again, in words, when no moment is known.
    pub(crate) fn until_without_a_time(self) -> Option<&'static str> {
        match self {
            LockWhy::GuardianLock => Some("When a guardian unlocks it"),
            LockWhy::HeldByAnotherDevice => Some("When that device is back home"),
            LockWhy::AwayTimeUsedUp => Some("When this phone is back home"),
            _ => None,
        }
    }

    /// What the lock says when it offers no way out: a line in bold and one
    /// under it.
    pub(crate) fn nothing_to_do(self) -> (&'static str, &'static str) {
        match self {
            LockWhy::WaitingForTime => ("Nothing to do here.", "The phone opens by itself."),
            LockWhy::AwayTimeUsedUp => ("Nothing to do here for now.", "More time comes when this phone is back home."),
            LockWhy::CannotCheck => ("Ask a guardian.", "A guardian can look at LiMo on the family PC to see what is wrong."),
            LockWhy::GuardianLock => ("A guardian can unlock it.", "From LiMo on their phone, or in LiMo on the family PC."),
            LockWhy::BlockedHours => ("Nothing to do here for now.", "The phone opens again by itself."),
            LockWhy::AppBlocked => ("Nothing to do here.", "This app is blocked by your family's rules."),
            LockWhy::TimeUsedUp | LockWhy::NoTimeToday | LockWhy::HeldByAnotherDevice => {
                ("Ask a guardian.", "A guardian can give more time from LiMo on their phone.")
            }
        }
    }
}

impl Way {
    /// The way out in a few words, and the line under it.
    pub(crate) fn words(self) -> (&'static str, &'static str) {
        match self {
            Way::Ask => ("Ask for more time", "A guardian sees your request in LiMo"),
            Way::Code => ("Type a one time code", "A guardian who is away can read you 8 digits"),
            Way::Pin => ("Guardian PIN", "For a guardian who is with you"),
        }
    }
}

/// The longest label of an app the block screen shows, in characters. A
/// longer one is cut to end with an ellipsis, which is one of these.
pub const MAX_LABEL_CHARS: usize = 60;

/// The characters that change the direction text is laid out in (Unicode's
/// `Bidi_Control`). A label could use them to make its words read as
/// something else, so they go with the control characters.
const BIDI_CONTROLS: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}',
    '\u{2067}', '\u{2068}', '\u{2069}',
];

/// An app's label made fit to show: whitespace trimmed from both ends and
/// each run of it inside made one space, control characters and direction
/// marks dropped, and at most [`MAX_LABEL_CHARS`] characters, a longer one
/// cut to end with an ellipsis. None when nothing is left. A label is the
/// app's own word, so it is cleaned as the PC cleans a program's
/// description.
pub(crate) fn shown_label(raw: &str) -> Option<String> {
    let mut kept: Vec<char> = Vec::with_capacity(MAX_LABEL_CHARS + 1);
    let mut space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            space = !kept.is_empty();
            continue;
        }
        if c.is_control() || BIDI_CONTROLS.contains(&c) {
            continue;
        }
        if space {
            kept.push(' ');
            space = false;
        }
        kept.push(c);
        // One more than fits says there is more: no need to read on.
        if kept.len() > MAX_LABEL_CHARS {
            break;
        }
    }
    if kept.len() > MAX_LABEL_CHARS {
        kept.truncate(MAX_LABEL_CHARS - 1);
        while kept.last() == Some(&' ') {
            kept.pop();
        }
        kept.push('\u{2026}');
    }
    if kept.is_empty() { None } else { Some(kept.into_iter().collect()) }
}

/// The block screen's title: "Game Launcher is not allowed.", or "This app
/// is not allowed." with no label to show.
pub(crate) fn block_title(label: Option<&str>) -> String {
    match label.and_then(shown_label) {
        Some(label) => format!("{label} is not allowed."),
        None => LockWhy::AppBlocked.title().to_string(),
    }
}

/// The words of the way back to a call in progress, on a cover.
pub(crate) const BACK_TO_THE_CALL: &str = "Back to the call";

/// The line under a way out that must wait: "Wait until 9:47 pm".
pub(crate) fn wait_row(now: i64, until: i64, clock: PhoneClock) -> String {
    format!("Wait until {}", until_in_sentence(now, until, clock))
}

/// The wait after wrong tries: "Too many wrong tries. The next try is at
/// 9:47 pm.", or "... is tomorrow, 7:00 am.".
pub(crate) fn wait_words(now: i64, until: i64, clock: PhoneClock) -> String {
    let at = until_in_sentence(now, until, clock);
    if at.starts_with(|c: char| c.is_ascii_digit()) {
        format!("Too many wrong tries. The next try is at {at}.")
    } else {
        format!("Too many wrong tries. The next try is {at}.")
    }
}

/// The newest request, as the lock says it: "You asked for 30 min at 9:32
/// pm", and how it stands: "Waiting", "Given 15 min" or "Not this time".
pub(crate) fn request_words(r: &RequestView, clock: PhoneClock) -> (String, String) {
    let line = format!("You asked for {} at {}", minutes(r.request.minutes), time_of_day(r.request.asked_at, clock));
    let stamp = match r.answer {
        RequestAnswer::Waiting => "Waiting".to_string(),
        RequestAnswer::Granted { minutes: given } => format!("Given {}", minutes(given)),
        RequestAnswer::Refused => "Not this time".to_string(),
    };
    (line, stamp)
}

// ---------- The time left and the warnings ----------

/// The time left display's words: "1 h 12 min left", "1 h 12 min paused",
/// "No limit today", or "Open until 9:30 pm" while a guardian's unlock
/// keeps the phone open.
pub(crate) fn time_left_words(
    seconds_left: Option<u64>,
    counting: bool,
    unlocked_until: Option<i64>,
    now: i64,
    clock: PhoneClock,
) -> String {
    if let Some(until) = unlocked_until.filter(|u| *u > now) {
        return format!("Open until {}", until_in_sentence(now, until, clock));
    }
    match seconds_left {
        None => "No limit today".to_string(),
        Some(s) if counting => format!("{} left", time_left(s)),
        Some(s) => format!("{} paused", time_left(s)),
    }
}

/// The minutes a warning says, for the warning point reached (`point`) with
/// `secs_left` seconds left until the lock: the time left in whole minutes,
/// rounded up, never less than 1 and never more than the point. When the
/// time left is the point, as it is on most days, that is the point itself.
pub(crate) fn minutes_to_say(point: u32, secs_left: i64) -> u32 {
    let secs = u64::try_from(secs_left).unwrap_or(0);
    let left = u32::try_from(secs.div_ceil(60)).unwrap_or(u32::MAX);
    left.min(point).max(1)
}

/// A warning's three lines: what is coming, a hint, and when.
pub(crate) struct WarningWords {
    pub(crate) message: String,
    pub(crate) hint: String,
    pub(crate) when: String,
}

/// The words of a warning given at `now`. `away` is true when the time
/// running out is this phone's time for away from home, and more comes when
/// it is back home.
pub(crate) fn warning_words(
    cause: WarningCause,
    point: u32,
    lock_at: i64,
    now: i64,
    away: bool,
    clock: PhoneClock,
) -> WarningWords {
    let said = minutes_to_say(point, lock_at.saturating_sub(now));
    let span = if said == 1 { "1 minute".to_string() } else { format!("{said} minutes") };
    let message = match cause {
        WarningCause::TimeRunsOut if away => format!("{span} of away time left today."),
        WarningCause::TimeRunsOut => format!("{span} of screen time left today."),
        WarningCause::BlockedHoursStart => format!("The phone locks in {span}, for the hours it is not used."),
        WarningCause::UnlockEnds => format!("The unlock ends in {span}, and the phone locks then."),
    };
    let hint = match (cause, point) {
        (_, 0..=1) => "Save what you are doing now.",
        (WarningCause::TimeRunsOut, 15..) if away => "More time comes when this phone is back home.",
        (WarningCause::TimeRunsOut, 15..) => "The phone locks when the time is used up.",
        (WarningCause::BlockedHoursStart, _) => "Finish what you are doing.",
        _ => "Save what you are doing.",
    };
    let at = until_in_sentence(now, lock_at, clock);
    let when = match cause {
        WarningCause::TimeRunsOut => format!("Locks about {at}"),
        _ => format!("Locks at {at}"),
    };
    WarningWords { message, hint: hint.to_string(), when }
}

// ---------- The answers to a code or a PIN ----------

/// What the phone says to a code or a PIN typed. `opens_now` is true when
/// the phone is open after a right one.
pub(crate) fn try_words(way: Way, outcome: &TryOutcome, now: i64, clock: PhoneClock) -> String {
    let what = if way == Way::Code { "code" } else { "PIN" };
    match outcome {
        TryOutcome::Added { added } => {
            let minutes = u32::try_from(added.secs.div_ceil(60)).unwrap_or(u32::MAX);
            format!("{} added.", capitalised(&long_minutes(minutes)))
        }
        TryOutcome::Unlocked { until } => format!("Open until {}.", until_in_sentence(now, *until, clock)),
        TryOutcome::ReleaseConfirmed => "That was right.".to_string(),
        TryOutcome::Wrong { wait_until: Some(until), .. } if *until > now => wait_words(now, *until, clock),
        TryOutcome::Wrong { wrong_guesses, .. } => {
            match limo_core::FREE_WRONG_GUESSES.saturating_sub(*wrong_guesses) {
                0 => format!("That {what} is not right."),
                1 => format!("That {what} is not right. 1 more try before a wait."),
                n => format!("That {what} is not right. {n} more tries before a wait."),
            }
        }
        TryOutcome::MustWait { until } => wait_words(now, *until, clock),
        TryOutcome::Malformed if way == Way::Code => "A code has 8 digits.".to_string(),
        TryOutcome::Malformed => "A PIN has 6 to 8 digits.".to_string(),
        TryOutcome::AlreadyUsed => "That code was already used today.".to_string(),
        TryOutcome::NotSet if way == Way::Code => "Codes are not set up on this phone yet.".to_string(),
        TryOutcome::NotSet => "A guardian PIN is not set up yet.".to_string(),
    }
}

/// Said under a right code or PIN when the phone is open after it.
pub(crate) const OPENS_NOW: &str = "The phone opens now.";

// ---------- Today and the rules ----------

/// A span of time used, in whole minutes rounded down, so that it never
/// says more was used than was: "0 min", "48 min", "2 h", "1 h 5 min".
/// Beside the time left, which is rounded up, the two add up to the day.
pub(crate) fn used_span(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The time left as Today shows it large: "1 h 12 min", "14 min", "0:42";
/// "No limit today"; or "Open until 9:30 pm" while a guardian's unlock
/// keeps the phone open.
pub(crate) fn today_left(
    seconds_left: Option<u64>,
    unlocked_until: Option<i64>,
    now: i64,
    clock: PhoneClock,
) -> (LeftKind, String) {
    if let Some(until) = unlocked_until.filter(|u| *u > now) {
        return (LeftKind::Open, format!("Open until {}", until_in_sentence(now, until, clock)));
    }
    match seconds_left {
        None => (LeftKind::NoLimit, "No limit today".to_string()),
        Some(s) => (LeftKind::Left, time_left(s)),
    }
}

/// The line under Today's bar, "48 min used of 2 h" ("48 min used today" on
/// a day with no limit, or with no time at all), and the part of the day's
/// time used, from 0 to 1 in thousandths, for the bar: None with no limit,
/// 1 on a day with no time.
pub(crate) fn used_words(used_secs: u64, budget_secs: Option<u64>) -> (String, Option<f64>) {
    let used = used_span(used_secs);
    match budget_secs {
        None => (format!("{used} used today"), None),
        Some(0) => (format!("{used} used today"), Some(1.0)),
        Some(budget) => {
            let part = (used_secs as f64 / budget as f64).min(1.0);
            (format!("{used} used of {}", used_span(budget)), Some((part * 1_000.0).round() / 1_000.0))
        }
    }
}

/// The dot between the parts of a line, with a space on each side.
const DOT: &str = " \u{b7} ";

/// Where today's time went: "On this phone 31 min". When the phone knows
/// how much was used on the child's other devices (a minute or more), it
/// says so too: "On this phone 31 min · on the PC 17 min" when all of it was
/// on the family PC, "On this phone 31 min · on other devices 17 min" when
/// it was on more than one, or the phone cannot tell which.
pub(crate) fn where_words(on_this_phone: u64, elsewhere_secs: Option<u64>, all_on_the_pc: bool) -> String {
    let here = format!("On this phone {}", used_span(on_this_phone));
    match elsewhere_secs.filter(|s| *s >= 60) {
        None => here,
        Some(secs) if all_on_the_pc => format!("{here}{DOT}on the PC {}", used_span(secs)),
        Some(secs) => format!("{here}{DOT}on other devices {}", used_span(secs)),
    }
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A blocked window as two moments, on the home date that starts at
/// `midnight`.
fn window_span(window: &BlockedWindow, midnight: i64) -> (i64, i64) {
    let start = midnight.saturating_add(i64::from(window.start_minute) * 60);
    (start, start.saturating_add(i64::from(window.length_minutes()) * 60))
}

/// Today's hours with screens off, on the phone's clock: "Screens are off
/// from 9:00 pm to 7:00 am.". These are the blocked hours that start on
/// today's home date (a window belongs to the day it starts on, as in the
/// rules), in order, with those that touch or overlap said as one: "Screens
/// are off from 12:00 pm to 1:00 pm and from 9:00 pm to 7:00 am.". "Screens
/// are off all day." when they cover the whole date. None when no hours are
/// off today.
pub(crate) fn bedtime_words(rules: &RuleSet, now: i64, clock: PhoneClock) -> Option<String> {
    let cal = rules.calendar();
    let date = cal.date_of(now);
    let weekday = Weekday::of_date(date);
    let midnight = cal.midnight(date);
    let mut spans: Vec<(i64, i64)> =
        rules.blocked_hours.iter().filter(|w| w.days.contains(&weekday)).map(|w| window_span(w, midnight)).collect();
    spans.sort_unstable();
    let mut joined: Vec<(i64, i64)> = Vec::new();
    for (start, end) in spans {
        match joined.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => joined.push((start, end)),
        }
    }
    if joined.is_empty() {
        return None;
    }
    let day_end = midnight.saturating_add(SECONDS_PER_DAY);
    if joined.iter().any(|(start, end)| *start <= midnight && *end >= day_end) {
        return Some("Screens are off all day.".to_string());
    }
    let parts: Vec<String> = joined
        .iter()
        .map(|(start, end)| format!("from {} to {}", time_of_day(*start, clock), time_of_day(*end, clock)))
        .collect();
    Some(format!("Screens are off {}.", and_list(&parts)))
}

/// The days of a rule, when it is not every day: "from Monday to Friday"
/// for three days or more in a row ("from Sunday to Thursday" across the
/// week's end), otherwise "on Saturday and Sunday" or "on Friday". None for
/// all seven days, and for none.
fn days_words(days: &[Weekday]) -> Option<String> {
    let mut on = days.to_vec();
    on.sort_unstable();
    on.dedup();
    let count = on.len();
    if count == 0 || count == Weekday::ALL.len() {
        return None;
    }
    let has = |i: usize| on.contains(&Weekday::ALL[i % 7]);
    // A single run starts on the one day whose day before is not in it.
    let run = (0..7).find(|i| has(*i) && !has(i + 6)).filter(|first| (0..count).all(|k| has(first + k)));
    match run {
        Some(first) if count >= 3 => Some(format!(
            "from {} to {}",
            Weekday::ALL[first].name(),
            Weekday::ALL[(first + count - 1) % 7].name()
        )),
        _ => Some(format!("on {}", and_list(&on.iter().map(|d| d.name().to_string()).collect::<Vec<_>>()))),
    }
}

/// The time a day, for How it works: "2 h", "No limit", "No screen time",
/// or, when the days differ, each with its days, in the week's order: "1 h
/// from Monday to Friday; 3 h on Saturday and Sunday".
pub(crate) fn time_a_day(rules: &RuleSet) -> String {
    let mut groups: Vec<(DailyBudget, Vec<Weekday>)> = Vec::new();
    for day in Weekday::ALL {
        let budget = rules.budgets.get(day);
        match groups.iter_mut().find(|(b, _)| *b == budget) {
            Some((_, days)) => days.push(day),
            None => groups.push((budget, vec![day])),
        }
    }
    let parts: Vec<String> = groups
        .into_iter()
        .map(|(budget, days)| {
            let time = match budget {
                DailyBudget::NoLimit => "no limit".to_string(),
                DailyBudget::Minutes(0) => "no screen time".to_string(),
                DailyBudget::Minutes(m) => used_span(u64::from(m) * 60),
            };
            match days_words(&days) {
                None => time,
                Some(when) => format!("{time} {when}"),
            }
        })
        .collect();
    capitalised(&parts.join("; "))
}

/// The hours screens are off, for How it works, on the phone's clock: "9:00
/// pm to 7:00 am", with its days when it is not every day ("9:00 pm to 7:00
/// am from Sunday to Thursday; 10:00 pm to 8:00 am on Friday and
/// Saturday"), earliest first, and "All day" for hours that last the whole
/// day. None with no blocked hours.
pub(crate) fn screens_off(rules: &RuleSet, now: i64, clock: PhoneClock) -> Option<String> {
    let cal = rules.calendar();
    let midnight = cal.midnight(cal.date_of(now));
    let mut groups: Vec<(&BlockedWindow, Vec<Weekday>)> = Vec::new();
    for window in rules.blocked_hours.iter().filter(|w| !w.days.is_empty()) {
        let same = |w: &BlockedWindow| w.start_minute == window.start_minute && w.length_minutes() == window.length_minutes();
        match groups.iter_mut().find(|(w, _)| same(w)) {
            Some((_, days)) => days.extend(window.days.iter().copied()),
            None => groups.push((window, window.days.clone())),
        }
    }
    if groups.is_empty() {
        return None;
    }
    groups.sort_by_key(|(w, _)| (w.start_minute, w.length_minutes()));
    let parts: Vec<String> = groups
        .iter()
        .map(|(window, days)| {
            let hours = if window.length_minutes() >= u32::from(limo_core::time::MINUTES_PER_DAY) {
                "all day".to_string()
            } else {
                let (start, end) = window_span(window, midnight);
                format!("{} to {}", time_of_day(start, clock), time_of_day(end, clock))
            };
            match days_words(days) {
                None => hours,
                Some(when) => format!("{hours} {when}"),
            }
        })
        .collect();
    Some(capitalised(&parts.join("; ")))
}

impl Permission {
    /// What it costs while it is switched off, in one sentence: for the
    /// child's Today screen, which then says a guardian is needed.
    pub fn off_words(self) -> &'static str {
        match self {
            Permission::Notifications => {
                "Notifications are switched off, so LiMo cannot show the time left or a warning."
            }
            Permission::Phone => "Phone access is switched off, so a lock could hide a call.",
            Permission::UsageAccess => "Usage access is switched off, so LiMo cannot count the time in each app.",
            Permission::Accessibility => "Accessibility is switched off, so LiMo cannot see which app is open.",
            Permission::Battery => "Android may put LiMo to sleep to save battery.",
            Permission::Protection => "Protection is switched off, so LiMo can be removed.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limo_core::{AddedBy, AddedTime, TimeRequest};

    const H12: PhoneClock = PhoneClock { utc_offset_secs: 0, hours_24: false };
    const H24: PhoneClock = PhoneClock { utc_offset_secs: 0, hours_24: true };
    /// Wednesday 18 November 2026, 00:00 UTC.
    const WED: i64 = 1_794_960_000;

    fn at(day: i64, hour: i64, minute: i64) -> i64 {
        WED + day * 86_400 + hour * 3_600 + minute * 60
    }

    #[test]
    fn times_are_said_on_the_phones_own_clock() {
        assert_eq!(time_of_day(at(0, 21, 42), H12), "9:42 pm");
        assert_eq!(time_of_day(at(0, 21, 42), H24), "21:42");
        assert_eq!(time_of_day(at(0, 0, 0), H12), "12:00 am");
        assert_eq!(time_of_day(at(0, 12, 0), H12), "12:00 pm");
        assert_eq!(time_of_day(at(0, 7, 5), H24), "07:05");
        let india = PhoneClock { utc_offset_secs: 19_800, hours_24: false };
        assert_eq!(time_of_day(at(0, 23, 30), india), "5:00 am");
        let now = at(0, 21, 42);
        assert_eq!(opens_again(now, at(0, 22, 30), H12), "Today, 10:30 pm");
        assert_eq!(opens_again(now, at(1, 7, 0), H12), "Tomorrow, 7:00 am");
        assert_eq!(opens_again(now, at(2, 7, 0), H12), "Friday, 7:00 am");
        assert_eq!(opens_again(now, at(9, 7, 0), H12), "Friday 27 November, 7:00 am");
        assert_eq!(opens_again(now, now - 5, H12), "In a moment");
        assert_eq!(opens_again(now, at(0, 21, 46) + 1, H12), "Today, 9:47 pm", "rounded up");
        assert_eq!(until_in_sentence(now, at(0, 21, 47), H12), "9:47 pm");
        assert_eq!(until_in_sentence(now, at(1, 7, 0), H24), "tomorrow, 07:00");
        for t in [i64::MIN, i64::MAX, 0, -1] {
            let wild = PhoneClock { utc_offset_secs: i32::MAX, hours_24: false };
            let _ = opens_again(t, i64::MAX, wild);
            let _ = until_in_sentence(i64::MIN, t, H12);
        }
    }

    #[test]
    fn spans_and_the_time_left() {
        assert_eq!(minutes(15), "15 min");
        assert_eq!(minutes(60), "1 hour");
        assert_eq!(minutes(90), "1 hour 30 min");
        assert_eq!(minutes(150), "2 hours 30 min");
        assert_eq!(long_minutes(1), "1 minute");
        assert_eq!(long_minutes(121), "2 hours 1 minute");
        assert_eq!(time_left(4_261), "1 h 12 min");
        assert_eq!(time_left(7_200), "2 h");
        assert_eq!(time_left(42), "0:42");
        assert_eq!(time_left_words(Some(4_320), true, None, WED, H12), "1 h 12 min left");
        assert_eq!(time_left_words(Some(4_320), false, None, WED, H12), "1 h 12 min paused");
        assert_eq!(time_left_words(None, true, None, WED, H12), "No limit today");
        assert_eq!(time_left_words(Some(0), true, Some(at(0, 21, 30)), at(0, 20, 0), H12), "Open until 9:30 pm");
        let _ = time_left(u64::MAX);
        let _ = minutes(u32::MAX);
    }

    #[test]
    fn every_lock_has_a_short_title_beside_its_message() {
        for why in LockWhy::ALL {
            let title = why.title();
            assert!(title.ends_with('.') && title.chars().count() <= 32, "{title}");
            assert_ne!(title, why.message());
            assert!(why.message().ends_with('.'));
            let (bold, line) = why.nothing_to_do();
            assert!(bold.ends_with('.') && line.ends_with('.'));
        }
        assert_eq!(LockWhy::from_core(LockReason::NeedsMaster, true), LockWhy::WaitingForTime);
        assert_eq!(LockWhy::from_core(LockReason::NeedsMaster, false), LockWhy::AwayTimeUsedUp);
    }

    #[test]
    fn a_warning_says_the_time_actually_left_with_a_hint_and_when() {
        let now = at(0, 21, 30);
        let w = warning_words(WarningCause::TimeRunsOut, 15, now + 15 * 60, now, false, H12);
        assert_eq!(
            (w.message.as_str(), w.hint.as_str(), w.when.as_str()),
            ("15 minutes of screen time left today.", "The phone locks when the time is used up.", "Locks about 9:45 pm")
        );
        let w = warning_words(WarningCause::TimeRunsOut, 15, now + 6 * 60, now, false, H12);
        assert_eq!(w.message, "6 minutes of screen time left today.");
        let w = warning_words(WarningCause::TimeRunsOut, 15, now + 15 * 60, now, true, H12);
        assert_eq!((w.message.as_str(), w.hint.as_str()), (
            "15 minutes of away time left today.",
            "More time comes when this phone is back home."
        ));
        let w = warning_words(WarningCause::BlockedHoursStart, 5, now + 5 * 60, now, false, H12);
        assert_eq!((w.message.as_str(), w.hint.as_str(), w.when.as_str()), (
            "The phone locks in 5 minutes, for the hours it is not used.",
            "Finish what you are doing.",
            "Locks at 9:35 pm"
        ));
        let w = warning_words(WarningCause::UnlockEnds, 1, now + 42, now, false, H12);
        assert_eq!((w.message.as_str(), w.hint.as_str()), (
            "The unlock ends in 1 minute, and the phone locks then.",
            "Save what you are doing now."
        ));
        assert_eq!(minutes_to_say(5, -30), 1);
        assert_eq!(minutes_to_say(u32::MAX, i64::MAX), u32::MAX);
    }

    #[test]
    fn the_answers_to_a_code_or_a_pin() {
        let now = at(0, 21, 42);
        let added = |secs| TryOutcome::Added {
            added: AddedTime {
                id: "p/code/1/00".into(),
                child_id: "c".into(),
                device_id: "p".into(),
                day: 1,
                secs,
                by: AddedBy::Pin,
                at: now,
            },
        };
        assert_eq!(try_words(Way::Code, &added(1_800), now, H12), "30 minutes added.");
        assert_eq!(try_words(Way::Pin, &added(3_600), now, H12), "1 hour added.");
        assert_eq!(try_words(Way::Pin, &TryOutcome::Unlocked { until: at(0, 22, 42) }, now, H12), "Open until 10:42 pm.");
        assert_eq!(try_words(Way::Pin, &TryOutcome::ReleaseConfirmed, now, H12), "That was right.");
        let wrong = |n| TryOutcome::Wrong { wrong_guesses: n, wait_until: None };
        assert_eq!(try_words(Way::Code, &wrong(1), now, H12), "That code is not right. 4 more tries before a wait.");
        assert_eq!(try_words(Way::Pin, &wrong(4), now, H12), "That PIN is not right. 1 more try before a wait.");
        assert_eq!(try_words(Way::Pin, &wrong(9), now, H12), "That PIN is not right.");
        let waited = TryOutcome::Wrong { wrong_guesses: 5, wait_until: Some(at(0, 21, 43)) };
        assert_eq!(try_words(Way::Code, &waited, now, H12), "Too many wrong tries. The next try is at 9:43 pm.");
        let must = TryOutcome::MustWait { until: at(1, 7, 0) };
        assert_eq!(try_words(Way::Pin, &must, now, H12), "Too many wrong tries. The next try is tomorrow, 7:00 am.");
        assert_eq!(try_words(Way::Code, &TryOutcome::Malformed, now, H12), "A code has 8 digits.");
        assert_eq!(try_words(Way::Pin, &TryOutcome::Malformed, now, H12), "A PIN has 6 to 8 digits.");
        assert_eq!(try_words(Way::Code, &TryOutcome::AlreadyUsed, now, H12), "That code was already used today.");
        assert_eq!(try_words(Way::Code, &TryOutcome::NotSet, now, H12), "Codes are not set up on this phone yet.");
        assert_eq!(try_words(Way::Pin, &TryOutcome::NotSet, now, H12), "A guardian PIN is not set up yet.");
    }

    #[test]
    fn a_request_and_its_answer() {
        let view = |answer| RequestView {
            request: TimeRequest {
                id: "p/ask/1".into(),
                child_id: "c".into(),
                device_id: "p".into(),
                day: 1,
                minutes: 30,
                reason: String::new(),
                asked_at: at(0, 21, 32),
            },
            answer,
            answered_at: None,
            reached_master: true,
        };
        assert_eq!(request_words(&view(RequestAnswer::Waiting), H12), (
            "You asked for 30 min at 9:32 pm".to_string(),
            "Waiting".to_string()
        ));
        assert_eq!(request_words(&view(RequestAnswer::Granted { minutes: 15 }), H12).1, "Given 15 min");
        assert_eq!(request_words(&view(RequestAnswer::Refused), H12).1, "Not this time");
        assert_eq!(wait_row(at(0, 21, 42), at(0, 21, 47), H12), "Wait until 9:47 pm");
        assert_eq!(Way::Pin.words(), ("Guardian PIN", "For a guardian who is with you"));
    }

    #[test]
    fn the_block_screen_names_the_app_by_its_label_cleaned() {
        assert_eq!(block_title(Some("Game Launcher")), "Game Launcher is not allowed.");
        assert_eq!(block_title(None), "This app is not allowed.");
        assert_eq!(LockWhy::AppBlocked.message(), "It is blocked by your family's rules.");
        // Trimmed, runs of space made one, control characters and direction
        // marks dropped.
        assert_eq!(shown_label("  Game \t\t Launcher \r\n"), Some("Game Launcher".into()));
        assert_eq!(shown_label("Game\nLauncher"), Some("Game Launcher".into()));
        assert_eq!(shown_label("Ga\u{0}me\u{7} Laun\u{1b}cher\u{7f}"), Some("Game Launcher".into()));
        assert_eq!(shown_label("\u{202E}rehcnuaL emaG"), Some("rehcnuaL emaG".into()));
        // Nothing left: no label, and the title says "This app".
        assert_eq!(shown_label(" \u{0}\t\u{7}\u{a0} "), None);
        assert_eq!(block_title(Some("\u{202E}\u{0} ")), "This app is not allowed.");
        // At most sixty characters, a longer one cut to end with an ellipsis.
        let sixty = "x".repeat(MAX_LABEL_CHARS);
        assert_eq!(shown_label(&format!("{sixty}   ")), Some(sixty.clone()));
        let cut = shown_label(&format!("{sixty}y")).unwrap();
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        assert_eq!(cut, format!("{}\u{2026}", "x".repeat(MAX_LABEL_CHARS - 1)));
        // A cut never leaves a space before the ellipsis.
        let words = format!("{} bcdef", "a".repeat(58));
        assert_eq!(shown_label(&words), Some(format!("{}\u{2026}", "a".repeat(58))));
        let _ = shown_label(&"\u{202E}".repeat(10_000));
        assert_eq!(BACK_TO_THE_CALL, "Back to the call");
    }

    #[test]
    fn todays_time_left_used_and_where_it_went() {
        let now = at(0, 16, 0);
        assert_eq!(today_left(Some(4_320), None, now, H12), (LeftKind::Left, "1 h 12 min".to_string()));
        assert_eq!(today_left(Some(14 * 60), None, now, H12).1, "14 min");
        assert_eq!(today_left(Some(42), None, now, H12).1, "0:42");
        assert_eq!(today_left(None, None, now, H12), (LeftKind::NoLimit, "No limit today".to_string()));
        let open = today_left(Some(0), Some(at(0, 21, 30)), now, H12);
        assert_eq!(open, (LeftKind::Open, "Open until 9:30 pm".to_string()));
        assert_eq!(today_left(Some(0), Some(at(0, 21, 30)), now, H24).1, "Open until 21:30");
        // An unlock that has ended is not said.
        assert_eq!(today_left(Some(60), Some(now - 1), now, H12).0, LeftKind::Left);

        // Used is rounded down and the time left up, so the two make the day.
        assert_eq!(used_span(0), "0 min");
        assert_eq!(used_span(59), "0 min");
        assert_eq!(used_span(48 * 60 + 59), "48 min");
        assert_eq!(used_span(7_200), "2 h");
        assert_eq!(used_span(3_900), "1 h 5 min");
        let _ = used_span(u64::MAX);
        let (words, part) = used_words(48 * 60, Some(7_200));
        assert_eq!((words.as_str(), part), ("48 min used of 2 h", Some(0.4)));
        assert_eq!(used_words(31 * 60 + 59, Some(7_200)), ("31 min used of 2 h".to_string(), Some(0.267)));
        assert_eq!(used_words(130 * 60, Some(7_200)), ("2 h 10 min used of 2 h".to_string(), Some(1.0)));
        assert_eq!(used_words(48 * 60, None), ("48 min used today".to_string(), None));
        assert_eq!(used_words(0, Some(0)), ("0 min used today".to_string(), Some(1.0)));
        assert_eq!(used_words(0, Some(5_400)).1, Some(0.0));

        assert_eq!(where_words(31 * 60, None, false), "On this phone 31 min");
        assert_eq!(where_words(31 * 60, Some(17 * 60), true), "On this phone 31 min \u{b7} on the PC 17 min");
        assert_eq!(
            where_words(3_900, Some(17 * 60 + 40), false),
            "On this phone 1 h 5 min \u{b7} on other devices 17 min"
        );
        assert_eq!(where_words(31 * 60, Some(59), true), "On this phone 31 min");
    }

    fn rules_with(windows: Vec<BlockedWindow>) -> RuleSet {
        RuleSet { blocked_hours: windows, ..RuleSet::new(1, 0) }
    }

    fn window(days: &[Weekday], start: (u16, u16), end: (u16, u16)) -> BlockedWindow {
        BlockedWindow { days: days.to_vec(), start_minute: start.0 * 60 + start.1, end_minute: end.0 * 60 + end.1 }
    }

    #[test]
    fn the_hours_screens_are_off_today() {
        // WED is a Wednesday; the home zone is UTC, as the phone's.
        let now = at(0, 16, 0);
        let bedtime = window(&Weekday::ALL, (21, 0), (7, 0));
        let rules = rules_with(vec![bedtime.clone()]);
        assert_eq!(bedtime_words(&rules, now, H12).as_deref(), Some("Screens are off from 9:00 pm to 7:00 am."));
        assert_eq!(bedtime_words(&rules, now, H24).as_deref(), Some("Screens are off from 21:00 to 07:00."));
        // On the phone's own clock, in its own zone.
        let india = PhoneClock { utc_offset_secs: 19_800, hours_24: false };
        assert_eq!(bedtime_words(&rules, now, india).as_deref(), Some("Screens are off from 2:30 am to 12:30 pm."));
        // Only the hours that start today; none at all is nothing.
        let thursdays = rules_with(vec![window(&[Weekday::Thursday], (21, 0), (7, 0))]);
        assert_eq!(bedtime_words(&thursdays, now, H12), None);
        assert_eq!(bedtime_words(&rules_with(vec![]), now, H12), None);
        // Several, in order; touching ones said as one.
        let school = window(&[Weekday::Wednesday], (8, 0), (13, 0));
        let lunch = window(&[Weekday::Wednesday], (13, 0), (14, 0));
        let two = rules_with(vec![bedtime.clone(), lunch.clone(), school]);
        assert_eq!(bedtime_words(&two, now, H12).as_deref(), Some("Screens are off from 8:00 am to 2:00 pm and from 9:00 pm to 7:00 am."));
        let three = rules_with(vec![bedtime.clone(), window(&[Weekday::Wednesday], (6, 0), (7, 0)), lunch]);
        assert_eq!(
            bedtime_words(&three, now, H12).as_deref(),
            Some("Screens are off from 6:00 am to 7:00 am, from 1:00 pm to 2:00 pm and from 9:00 pm to 7:00 am.")
        );
        let whole = rules_with(vec![window(&[Weekday::Wednesday], (0, 0), (24, 0)), bedtime]);
        assert_eq!(bedtime_words(&whole, now, H12).as_deref(), Some("Screens are off all day."));
        for t in [i64::MIN, i64::MAX] {
            let _ = bedtime_words(&two, t, H12);
            let _ = screens_off(&two, t, H12);
        }
    }

    #[test]
    fn the_rules_in_words() {
        let now = at(0, 16, 0);
        let mut rules = RuleSet::new(1, 0);
        assert_eq!(time_a_day(&rules), "2 h");
        assert_eq!(screens_off(&rules, now, H12), None);
        rules.budgets = limo_core::WeekBudgets::every_day(DailyBudget::NoLimit);
        assert_eq!(time_a_day(&rules), "No limit");
        rules.budgets = limo_core::WeekBudgets::every_day(DailyBudget::Minutes(90));
        assert_eq!(time_a_day(&rules), "1 h 30 min");
        rules.budgets.saturday = DailyBudget::Minutes(180);
        rules.budgets.sunday = DailyBudget::Minutes(180);
        assert_eq!(time_a_day(&rules), "1 h 30 min from Monday to Friday; 3 h on Saturday and Sunday");
        rules.budgets.sunday = DailyBudget::NoLimit;
        rules.budgets.wednesday = DailyBudget::Minutes(0);
        assert_eq!(
            time_a_day(&rules),
            "1 h 30 min on Monday, Tuesday, Thursday and Friday; no screen time on Wednesday; 3 h on Saturday; no limit on Sunday"
        );

        let school_nights = [Weekday::Sunday, Weekday::Monday, Weekday::Tuesday, Weekday::Wednesday, Weekday::Thursday];
        rules.blocked_hours = vec![window(&Weekday::ALL, (21, 0), (7, 0))];
        assert_eq!(screens_off(&rules, now, H12).as_deref(), Some("9:00 pm to 7:00 am"));
        assert_eq!(screens_off(&rules, now, H24).as_deref(), Some("21:00 to 07:00"));
        rules.blocked_hours = vec![
            window(&[Weekday::Friday, Weekday::Saturday], (22, 0), (8, 0)),
            window(&school_nights[..1], (21, 0), (7, 0)),
            window(&school_nights[1..], (21, 0), (7, 0)),
            window(&[Weekday::Sunday], (0, 0), (24, 0)),
            window(&[], (12, 0), (13, 0)),
        ];
        // Grouped by their hours, earliest first, school nights across the
        // week's end, a window with no days left out.
        assert_eq!(
            screens_off(&rules, now, H12).as_deref(),
            Some("All day on Sunday; 9:00 pm to 7:00 am from Sunday to Thursday; 10:00 pm to 8:00 am on Friday and Saturday")
        );
        assert_eq!(days_words(&[Weekday::Saturday, Weekday::Sunday, Weekday::Tuesday]).as_deref(), Some("on Tuesday, Saturday and Sunday"));
        assert_eq!(days_words(&[Weekday::Friday, Weekday::Saturday, Weekday::Sunday, Weekday::Monday]).as_deref(), Some("from Friday to Monday"));
        assert_eq!(days_words(&[Weekday::Friday]).as_deref(), Some("on Friday"));
        assert_eq!(days_words(&[]), None);
    }

    #[test]
    fn what_each_permission_switched_off_costs() {
        let said: Vec<&str> = Permission::ALL.iter().map(|p| p.off_words()).collect();
        assert_eq!(
            said,
            [
                "Notifications are switched off, so LiMo cannot show the time left or a warning.",
                "Phone access is switched off, so a lock could hide a call.",
                "Usage access is switched off, so LiMo cannot count the time in each app.",
                "Accessibility is switched off, so LiMo cannot see which app is open.",
                "Android may put LiMo to sleep to save battery.",
                "Protection is switched off, so LiMo can be removed.",
            ]
        );
    }
}
