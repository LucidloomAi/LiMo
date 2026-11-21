//! The child's own screens in the app, as plain data with every word in
//! them: Today (the time left, the time used, where it went, the hours
//! screens are off, the minutes of each app, and whether a guardian is
//! needed) and the rules in words, for How it works. Both are in the view
//! ([`crate::PhoneView`]); the sentences themselves are in `words.rs`.
//!
//! **An app is its package name here, never its label.** The Android side
//! puts each app's name in before the screens show it.
//!
//! **What the phone knows of the child's other devices.** Their use that
//! day, as the master counted it when it made the newest grant the phone
//! took for the day (the core's `Grant::others_used_secs`), and whether all
//! of it was on the family PC. Today's "used" line is then the child's whole
//! day, this phone's own use and theirs, and "where used" says both: "On
//! this phone 31 min · on the PC 17 min". The figure is what the master
//! knew then; away from home it stays the best known one. Until a grant for
//! the day says it (a phone just paired with no use elsewhere, a master
//! that does not say), Today says this phone's own use only. It never
//! guesses the rest.

use limo_core::{Decision, OthersUsed, RuleSet};
use serde::{Deserialize, Serialize};

use crate::facts::{Permission, PhonePermissions};
use crate::kept::AppUse;
use crate::words::{self, PhoneClock};

/// The child's Today screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TodayView {
    /// The time left today, shown large.
    pub left: TodayLeft,
    /// The line under the bar: "48 min used of 2 h", or "48 min used today"
    /// on a day with no limit. The child's whole day as far as the phone
    /// knows: its own counted use and, once the master has said it, the use
    /// on the child's other devices (see the module's notes).
    pub used: String,
    /// The part of today's time used, from 0 to 1, for the bar; None on a
    /// day with no limit, where there is no bar.
    pub used_part: Option<f64>,
    /// Where the time went: "On this phone 31 min", and once the master has
    /// said a minute or more went elsewhere, "On this phone 31 min · on the
    /// PC 17 min" (all of it on the family PC) or "On this phone 31 min · on
    /// other devices 17 min".
    pub where_used: String,
    /// "Screens are off from 9:00 pm to 7:00 am.", or None when no hours
    /// are off today.
    pub bedtime: Option<String>,
    /// Today on this phone: each app with a minute or more, longest first.
    pub apps: Vec<TodayApp>,
    /// Something LiMo needs is switched off, so a guardian is needed. None
    /// while everything is on.
    pub guardian_needed: Option<GuardianNeeded>,
}

/// The time left today, as Today shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodayLeft {
    /// Which kind.
    pub kind: LeftKind,
    /// "1 h 12 min", "14 min" or "0:42"; "No limit today"; "Open until 9:30
    /// pm".
    pub words: String,
}

/// What kind of time left Today shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftKind {
    /// Time left, with "Left today" under it.
    Left,
    /// Today has no limit.
    NoLimit,
    /// A guardian's unlock keeps the phone open.
    Open,
}

/// One app on Today's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodayApp {
    /// Its package name; the Android side shows its name.
    pub package: String,
    /// Its use today, in seconds, whether its time counts or not.
    pub secs: u64,
    /// The same in words: "12 min", "1 h 5 min".
    pub minutes: String,
}

/// What is switched off, for Today's banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianNeeded {
    /// Each permission that is off, in the order setup asks for them. Never
    /// empty.
    pub off: Vec<SwitchedOff>,
}

/// One permission switched off, with what that costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchedOff {
    /// Which.
    pub permission: Permission,
    /// "Accessibility is switched off, so LiMo cannot see which app is
    /// open."
    pub words: String,
}

/// The child's rules in words, for How it works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesWords {
    /// "2 h", "No limit", or each day's time when the days differ.
    pub time_a_day: String,
    /// "9:00 pm to 7:00 am", or None when no hours are off.
    pub screens_off: Option<String>,
    /// The apps that stay open under a lock, by package name.
    pub always_open: Vec<String>,
    /// The apps that are not allowed, by package name.
    pub not_allowed: Vec<String>,
}

/// The day's use as the phone knows it: its own counted use, and the use on
/// the child's other devices when the master has said it for the day.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Used {
    /// This phone's counted use on its day, reported or not.
    pub(crate) here_secs: u64,
    /// The other devices' use that day, as the newest grant for it said.
    pub(crate) others: Option<OthersUsed>,
}

/// Today, from the decision of the last call and the day's use.
pub(crate) fn today(
    rules: &RuleSet,
    decision: &Decision,
    use_today: Used,
    apps: Vec<AppUse>,
    permissions: Option<PhonePermissions>,
    now: i64,
    clock: PhoneClock,
) -> TodayView {
    let Used { here_secs, others } = use_today;
    let others = others.filter(|o| o.day == decision.day);
    let whole_day = here_secs.saturating_add(others.map_or(0, |o| o.secs));
    let (kind, left) = words::today_left(decision.seconds_left, decision.unlocked_until, now, clock);
    let (used, used_part) = words::used_words(whole_day, rules.budget_secs_on(decision.day));
    let mut apps: Vec<TodayApp> = apps
        .into_iter()
        .filter(|a| a.secs >= 60)
        .map(|a| TodayApp { minutes: words::used_span(a.secs), package: a.package, secs: a.secs })
        .collect();
    apps.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| a.package.cmp(&b.package)));
    TodayView {
        left: TodayLeft { kind, words: left },
        used,
        used_part,
        // What the master said of the other devices: see the module's notes.
        where_used: words::where_words(
            here_secs,
            others.map(|o| o.secs),
            others.is_some_and(|o| o.all_on_master),
        ),
        bedtime: words::bedtime_words(rules, now, clock),
        apps,
        guardian_needed: guardian_needed(permissions),
    }
}

/// What is switched off, from the permissions the last call saw; None while
/// all are on, and before any call.
pub(crate) fn guardian_needed(permissions: Option<PhonePermissions>) -> Option<GuardianNeeded> {
    let off: Vec<SwitchedOff> = permissions?
        .off()
        .into_iter()
        .map(|permission| SwitchedOff { permission, words: permission.off_words().to_string() })
        .collect();
    (!off.is_empty()).then_some(GuardianNeeded { off })
}

/// The rules in words, with times on the phone's clock.
pub(crate) fn rules_words(rules: &RuleSet, now: i64, clock: PhoneClock) -> RulesWords {
    RulesWords {
        time_a_day: words::time_a_day(rules),
        screens_off: words::screens_off(rules, now, clock),
        always_open: rules.always_allowed_apps.clone(),
        not_allowed: rules.blocked_apps.clone(),
    }
}
