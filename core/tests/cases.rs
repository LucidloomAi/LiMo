//! Loads `tests/cases.toml`, the file of cases a product owner can read and
//! extend, and checks every case against the library. A wrong expectation
//! fails with the case's name. The README says how to add a case.

use std::collections::BTreeMap;

use limo_core::{
    BlockedHours, BlockedWindow, CivilDate, DailyBudget, Decision, Holding, HomeCalendar,
    LockReason, MasterWord, RuleSet, Warned, WarningCause, Weekday, decide,
};
use serde::Deserialize;

const CASE_FILE: &str = include_str!("cases.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    #[serde(default)]
    rules: BTreeMap<String, RulesSpec>,
    #[serde(default)]
    case: Vec<Case>,
    #[serde(default)]
    rules_check: Vec<RulesCheck>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesSpec {
    version: Option<u32>,
    home_zone: Option<String>,
    day_starts: Option<String>,
    budget: Option<BTreeMap<String, BudgetValue>>,
    #[serde(default)]
    blocked: Vec<WindowSpec>,
    #[serde(default)]
    always_allowed: Vec<String>,
    #[serde(default)]
    blocked_apps: Vec<String>,
    away_share: Option<u8>,
    lease_minutes: Option<u32>,
    warnings: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum BudgetValue {
    Minutes(u32),
    Text(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowSpec {
    days: Vec<String>,
    from: String,
    to: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    rules: String,
    time: Option<String>,
    device_time: Option<String>,
    app: Option<String>,
    holding_day: Option<String>,
    used: Option<String>,
    lease_left: Option<String>,
    away_left: Option<String>,
    away_percent: Option<u8>,
    master_left: Option<String>,
    master_held_elsewhere: Option<String>,
    #[serde(default)]
    warned_time: Vec<u32>,
    #[serde(default)]
    warned_blocked: Vec<u32>,
    expect: Expect,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    allowed: Option<bool>,
    reason: Option<String>,
    device_reason: Option<String>,
    until: Option<String>,
    left: Option<String>,
    in_hand: Option<String>,
    warning: Option<String>,
    next_check: Option<String>,
    counts: Option<bool>,
    ask_master: Option<bool>,
    warned_time: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesCheck {
    name: String,
    rules: String,
    problems: Vec<String>,
}

// Reading the plain words of the file.

/// "+05:30" or "-05:00" to seconds.
fn zone(text: &str) -> Result<i32, String> {
    let t = text.trim();
    let (sign, rest) = match t.chars().next() {
        Some('+') => (1, &t[1..]),
        Some('-') => (-1, &t[1..]),
        _ => return Err(format!("a time zone looks like +05:30 or -05:00, not {text:?}")),
    };
    let minute = clock(rest)? as i32;
    Ok(sign * minute * 60)
}

/// "21:00" to minute 1260; "24:00" is 1440.
fn clock(text: &str) -> Result<u16, String> {
    let (h, m) = text
        .trim()
        .split_once(':')
        .ok_or_else(|| format!("a time of day looks like 21:00, not {text:?}"))?;
    let h: u16 = h.parse().map_err(|_| format!("bad hour in {text:?}"))?;
    let m: u16 = m.parse().map_err(|_| format!("bad minute in {text:?}"))?;
    if m > 59 || h > 24 || (h == 24 && m > 0) {
        return Err(format!("no such time of day: {text:?}"));
    }
    Ok(h * 60 + m)
}

/// "2026-11-18" to a date.
fn civil(text: &str) -> Result<CivilDate, String> {
    let parts: Vec<&str> = text.split('-').collect();
    let bad = || format!("a date looks like 2026-11-18, not {text:?}");
    if parts.len() != 3 {
        return Err(bad());
    }
    let year = parts[0].parse().map_err(|_| bad())?;
    let month = parts[1].parse().map_err(|_| bad())?;
    let day = parts[2].parse().map_err(|_| bad())?;
    CivilDate::new(year, month, day).ok_or_else(bad)
}

/// Splits off a leading weekday ("Wed") and checks it against the date, so
/// that a slip in either is caught.
fn checked_date(words: &[&str]) -> Result<(CivilDate, usize), String> {
    match Weekday::from_name(words[0]) {
        Some(weekday) => {
            let date = civil(words.get(1).ok_or("a date is missing after the weekday")?)?;
            if date.weekday() != weekday {
                return Err(format!(
                    "{} {} is a {}, not a {}",
                    words[0],
                    words[1],
                    date.weekday().name(),
                    weekday.name()
                ));
            }
            Ok((date, 2))
        }
        None => Ok((civil(words[0])?, 1)),
    }
}

/// "20:55" or "20:55:30" to (hour, minute, second).
fn time_of_day(text: &str) -> Result<(u8, u8, u8), String> {
    let parts: Vec<&str> = text.split(':').collect();
    let bad = || format!("a time looks like 20:55 or 20:55:30, not {text:?}");
    if parts.len() < 2 || parts.len() > 3 {
        return Err(bad());
    }
    let h: u8 = parts[0].parse().map_err(|_| bad())?;
    let m: u8 = parts[1].parse().map_err(|_| bad())?;
    let s: u8 = if parts.len() == 3 { parts[2].parse().map_err(|_| bad())? } else { 0 };
    if h > 23 || m > 59 || s > 59 {
        return Err(bad());
    }
    Ok((h, m, s))
}

/// "Wed 2026-11-18 20:55" in home time to unix seconds.
fn home_time(cal: &HomeCalendar, text: &str) -> Result<i64, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Err("an empty time".into());
    }
    let (date, used) = checked_date(&words)?;
    if words.len() != used + 1 {
        return Err(format!("a home time looks like \"Wed 2026-11-18 20:55\", not {text:?}"));
    }
    let (h, m, s) = time_of_day(words[used])?;
    Ok(cal.at(date, h, m, s))
}

/// "2026-11-18 10:25 -05:00", the clock of a device where it is, to unix
/// seconds.
fn device_time(text: &str) -> Result<i64, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Err("an empty time".into());
    }
    let (date, used) = checked_date(&words)?;
    if words.len() != used + 2 {
        return Err(format!("a device time looks like \"2026-11-18 10:25 -05:00\", not {text:?}"));
    }
    let (h, m, s) = time_of_day(words[used])?;
    let offset = zone(words[used + 1])?;
    Ok(HomeCalendar::new(offset, 0).at(date, h, m, s))
}

/// "Tue 2026-11-17" to the home day that starts on that date.
fn home_day(text: &str) -> Result<i64, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Err("an empty day".into());
    }
    let (date, used) = checked_date(&words)?;
    if words.len() != used {
        return Err(format!("a day looks like \"Tue 2026-11-17\", not {text:?}"));
    }
    Ok(date.date_number())
}

/// "4m", "90s", "1h 30m", "4m 30s" to seconds; "no limit" to None.
fn duration(text: &str) -> Result<Option<u64>, String> {
    let t = text.trim();
    if t == "no limit" {
        return Ok(None);
    }
    let mut total = 0u64;
    for word in t.split_whitespace() {
        let (number, unit) = word.split_at(word.find(|c: char| !c.is_ascii_digit()).unwrap_or(word.len()));
        let n: u64 = number.parse().map_err(|_| format!("bad duration {text:?}"))?;
        total += n * match unit {
            "h" => 3_600,
            "m" => 60,
            "s" => 1,
            _ => return Err(format!("a duration looks like 4m, 90s or 1h 30m, not {text:?}")),
        };
    }
    if t.is_empty() {
        return Err("an empty duration".into());
    }
    Ok(Some(total))
}

fn secs(text: &Option<String>) -> Result<u64, String> {
    match text {
        None => Ok(0),
        Some(t) => duration(t)?.ok_or_else(|| format!("{t:?} is not an amount of time")),
    }
}

fn reason_name(reason: Option<LockReason>) -> &'static str {
    match reason {
        None => "none",
        Some(LockReason::AppBlocked) => "app blocked",
        Some(LockReason::BlockedHours) => "blocked hours",
        Some(LockReason::NoTimeToday) => "no time today",
        Some(LockReason::TimeUsedUp) => "time used up",
        Some(LockReason::NeedsMaster) => "needs master",
        Some(LockReason::HeldByAnotherDevice) => "held by another device",
    }
}

fn duration_text(secs: Option<u64>) -> String {
    match secs {
        None => "no limit".into(),
        Some(s) if s % 60 == 0 => format!("{}m", s / 60),
        Some(s) => format!("{}m {}s", s / 60, s % 60),
    }
}

fn time_text(cal: &HomeCalendar, t: Option<i64>) -> String {
    let Some(t) = t else { return "never".into() };
    let l = cal.local(t);
    let weekday = &l.weekday.name()[..3];
    let date = format!("{:04}-{:02}-{:02}", l.date.year, l.date.month, l.date.day);
    if l.second == 0 {
        format!("{weekday} {date} {:02}:{:02}", l.hour, l.minute)
    } else {
        format!("{weekday} {date} {:02}:{:02}:{:02}", l.hour, l.minute, l.second)
    }
}

fn warning_text(d: &Decision) -> String {
    match &d.warning {
        None => "none".into(),
        Some(w) => format!(
            "{} {} before {}",
            w.minutes,
            if w.minutes == 1 { "minute" } else { "minutes" },
            match w.cause {
                WarningCause::TimeRunsOut => "time runs out",
                WarningCause::BlockedHoursStart => "blocked hours",
            }
        ),
    }
}

fn build_rules(name: &str, spec: &RulesSpec) -> Result<RuleSet, String> {
    let offset = match &spec.home_zone {
        Some(z) => zone(z)?,
        None => 0,
    };
    let mut rules = RuleSet::new(spec.version.unwrap_or(1), offset);
    if let Some(start) = &spec.day_starts {
        rules.day_start_minute = clock(start)?;
    }
    if let Some(budget) = &spec.budget {
        let value = |v: &BudgetValue| match v {
            BudgetValue::Minutes(m) => Ok(DailyBudget::Minutes(*m)),
            BudgetValue::Text(t) if t == "no limit" => Ok(DailyBudget::NoLimit),
            BudgetValue::Text(t) => Err(format!("rules {name}: a budget is minutes or \"no limit\", not {t:?}")),
        };
        if let Some(all) = budget.get("every_day") {
            rules.budgets = limo_core::WeekBudgets::every_day(value(all)?);
        }
        for (key, v) in budget {
            if key == "every_day" {
                continue;
            }
            let day = Weekday::from_name(key)
                .ok_or_else(|| format!("rules {name}: {key:?} is not a weekday or every_day"))?;
            rules.budgets.set(day, value(v)?);
        }
    }
    for w in &spec.blocked {
        let mut days = Vec::new();
        for d in &w.days {
            if d == "every day" {
                days.extend(Weekday::ALL);
            } else {
                days.push(Weekday::from_name(d).ok_or_else(|| format!("rules {name}: {d:?} is not a weekday"))?);
            }
        }
        rules.blocked_hours.push(BlockedWindow { days, start_minute: clock(&w.from)?, end_minute: clock(&w.to)? });
    }
    rules.always_allowed_apps = spec.always_allowed.clone();
    rules.blocked_apps = spec.blocked_apps.clone();
    if let Some(share) = spec.away_share {
        rules.away_share_percent = share;
    }
    if let Some(lease) = spec.lease_minutes {
        rules.lease_minutes = lease;
    }
    if let Some(w) = &spec.warnings {
        rules.warning_minutes = w.clone();
    }
    Ok(rules)
}

/// Checks one case and returns what went wrong, each line naming the case.
fn check_case(file: &CaseFile, case: &Case) -> Vec<String> {
    match run_case(file, case) {
        Ok(failures) => failures.into_iter().map(|f| format!("case {:?}: {f}", case.name)).collect(),
        Err(e) => vec![format!("case {:?}: cannot be read: {e}", case.name)],
    }
}

fn run_case(file: &CaseFile, case: &Case) -> Result<Vec<String>, String> {
    let spec = file.rules.get(&case.rules).ok_or_else(|| format!("no rules called {:?}", case.rules))?;
    let rules = build_rules(&case.rules, spec)?;
    let problems = rules.validate();
    if !problems.is_empty() {
        let words: Vec<String> = problems.into_iter().map(|p| p.message).collect();
        return Err(format!("its rules {:?} have problems: {}", case.rules, words.join(" ")));
    }
    let cal = rules.calendar();
    let now = match (&case.time, &case.device_time) {
        (Some(t), None) => home_time(&cal, t)?,
        (None, Some(t)) => device_time(t)?,
        _ => return Err("give either time (home time) or device_time, not both".into()),
    };
    let today = cal.day_of(now);
    let day = match &case.holding_day {
        Some(d) => home_day(d)?,
        None => today,
    };
    let used = secs(&case.used)?;
    let lease_left = secs(&case.lease_left)?;
    let blocked_start = match rules.blocked_hours_at(now) {
        BlockedHours::Open { next_start } => next_start,
        BlockedHours::Blocked { .. } => None,
    };
    let holding = Holding {
        day,
        lease_total_secs: used + lease_left,
        lease_used_secs: used,
        away_cap_secs: secs(&case.away_left)?,
        away_used_secs: 0,
        away_percent: case.away_percent.unwrap_or(0),
        master_word: match &case.master_left {
            Some(_) => Some(MasterWord {
                left_secs: secs(&case.master_left)?,
                used_then_secs: used,
                held_elsewhere_secs: secs(&case.master_held_elsewhere)?,
            }),
            None => None,
        },
        warned: Warned {
            time_day: (!case.warned_time.is_empty()).then_some(day),
            time_points: case.warned_time.clone(),
            blocked_start: if case.warned_blocked.is_empty() { None } else { blocked_start },
            blocked_points: case.warned_blocked.clone(),
        },
    };
    let d = decide(&rules, &holding, now, case.app.as_deref());

    let mut failures = Vec::new();
    let e = &case.expect;
    let mut compare = |what: &str, expected: &str, got: String| {
        if expected.trim() != got {
            failures.push(format!("{what}: expected {expected:?}, got {got:?}"));
        }
    };
    if let Some(v) = e.allowed {
        compare("allowed", &v.to_string(), d.allowed.to_string());
    }
    if let Some(v) = &e.reason {
        compare("reason", v, reason_name(d.lock.map(|l| l.reason)).into());
    }
    if let Some(v) = &e.device_reason {
        compare("device_reason", v, reason_name(d.device_lock.map(|l| l.reason)).into());
    }
    if let Some(v) = &e.until {
        let expected = if v == "never" { "never".to_string() } else { time_text(&cal, Some(home_time(&cal, v)?)) };
        match d.lock {
            Some(lock) => compare("until", &expected, time_text(&cal, lock.until)),
            None => compare("until", &expected, "not locked".into()),
        }
    }
    if let Some(v) = &e.left {
        compare("left", &duration_text(duration(v)?), duration_text(d.seconds_left));
    }
    if let Some(v) = &e.in_hand {
        compare("in_hand", &duration_text(duration(v)?), duration_text(d.seconds_in_hand));
    }
    if let Some(v) = &e.warning {
        compare("warning", v, warning_text(&d));
    }
    if let Some(v) = &e.next_check {
        compare("next_check", &time_text(&cal, Some(home_time(&cal, v)?)), time_text(&cal, Some(d.next_check_at)));
    }
    if let Some(v) = e.counts {
        compare("counts", &v.to_string(), d.counts.to_string());
    }
    if let Some(v) = e.ask_master {
        compare("ask_master", &v.to_string(), d.ask_master.to_string());
    }
    if let Some(v) = &e.warned_time {
        compare("warned_time", &format!("{v:?}"), format!("{:?}", d.warned.time_points));
    }
    Ok(failures)
}

fn check_rules(file: &CaseFile, check: &RulesCheck) -> Vec<String> {
    let name = &check.name;
    let Some(spec) = file.rules.get(&check.rules) else {
        return vec![format!("rules check {name:?}: no rules called {:?}", check.rules)];
    };
    let rules = match build_rules(&check.rules, spec) {
        Ok(r) => r,
        Err(e) => return vec![format!("rules check {name:?}: cannot be read: {e}")],
    };
    let messages: Vec<String> = rules.validate().into_iter().map(|p| p.message).collect();
    let mut failures = Vec::new();
    for wanted in &check.problems {
        if !messages.iter().any(|m| m.contains(wanted.as_str())) {
            failures.push(format!("rules check {name:?}: no problem says {wanted:?}; the problems were {messages:?}"));
        }
    }
    if messages.len() != check.problems.len() {
        failures.push(format!(
            "rules check {name:?}: expected {} problems, found {}: {messages:?}",
            check.problems.len(),
            messages.len()
        ));
    }
    failures
}

fn load() -> CaseFile {
    toml::from_str(CASE_FILE).unwrap_or_else(|e| panic!("tests/cases.toml cannot be read: {e}"))
}

#[test]
fn every_case_in_the_file_holds() {
    let file = load();
    let mut failures = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for case in &file.case {
        if !names.insert(case.name.clone()) {
            failures.push(format!("case {:?} appears twice", case.name));
        }
        failures.extend(check_case(&file, case));
    }
    for check in &file.rules_check {
        failures.extend(check_rules(&file, check));
    }
    assert!(
        failures.is_empty(),
        "{} problems in tests/cases.toml:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(file.case.len() >= 40, "the file must hold at least 40 cases, it holds {}", file.case.len());
    println!("{} cases and {} rules checks hold", file.case.len(), file.rules_check.len());
}

#[test]
fn a_wrong_expectation_fails_with_the_case_name() {
    let file = load();
    let mut case = file
        .case
        .iter()
        .find(|c| c.expect.allowed.is_some())
        .expect("a case with an expected answer")
        .clone();
    case.expect.allowed = Some(!case.expect.allowed.unwrap());
    let failures = check_case(&file, &case);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains(&case.name), "{}", failures[0]);
    assert!(failures[0].contains("allowed"), "{}", failures[0]);

    // A case that cannot be read also fails with its name.
    let mut broken = case.clone();
    broken.time = Some("Thu 2026-11-18 20:55".into());
    broken.device_time = None;
    let failures = check_case(&file, &broken);
    assert!(failures[0].contains(&broken.name) && failures[0].contains("is a Wednesday"), "{failures:?}");
}

#[test]
fn the_reader_understands_the_plain_words() {
    assert_eq!(zone("+05:30"), Ok(19_800));
    assert_eq!(zone("-05:00"), Ok(-18_000));
    assert_eq!(clock("24:00"), Ok(1_440));
    assert!(clock("24:01").is_err());
    assert_eq!(duration("1h 30m"), Ok(Some(5_400)));
    assert_eq!(duration("4m 30s"), Ok(Some(270)));
    assert_eq!(duration("no limit"), Ok(None));
    assert!(duration("4 minutes").is_err());
    let india = HomeCalendar::new(19_800, 0);
    let t = home_time(&india, "Wed 2026-11-18 20:55").unwrap();
    assert_eq!(device_time("2026-11-18 10:25 -05:00"), Ok(t));
    assert_eq!(time_text(&india, Some(t)), "Wed 2026-11-18 20:55");
    assert!(home_time(&india, "Mon 2026-11-18 20:55").is_err());
}
